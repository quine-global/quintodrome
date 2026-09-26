mod server;

use server::{Navidrome, ServerState};
use tauri::{
    Emitter, LogicalPosition, LogicalSize, Manager, RunEvent, Url, WebviewBuilder, WebviewUrl,
    WindowEvent,
};
use tauri_plugin_opener::OpenerExt;

const TOOLBAR_HEIGHT: f64 = 60.0;
/// Fallback logical window size, matching `tauri.conf.json`'s configured
/// window dimensions. Used when the window's real geometry isn't known yet
/// (e.g. on Linux/Wayland, where `inner_size()` can report 0x0 before the
/// window has been realized by the compositor).
const DEFAULT_WIDTH: f64 = 1280.0;
const DEFAULT_HEIGHT: f64 = 800.0;
const DEFAULT_PUBLIC_URL: &str = "http://quintodrome";

/// Maps the internal Navidrome origin (e.g. `http://127.0.0.1:4533`) to a
/// friendlier, user-facing origin shown in the address bar.
#[derive(Clone)]
struct UrlMask {
    real: String,
    public: String,
}

impl UrlMask {
    fn new(host: &str, port: u16) -> Self {
        let real = format!("http://{host}:{port}");
        let public = std::env::var("QUINTODROME_PUBLIC_URL")
            .unwrap_or_else(|_| DEFAULT_PUBLIC_URL.to_string());
        Self { real, public }
    }

    fn mask(&self, url: &str) -> String {
        url.strip_prefix(&self.real)
            .map(|rest| format!("{}{}", self.public, rest))
            .unwrap_or_else(|| url.to_string())
    }

    fn unmask(&self, url: &str) -> String {
        url.strip_prefix(&self.public)
            .map(|rest| format!("{}{}", self.real, rest))
            .unwrap_or_else(|| url.to_string())
    }
}

#[tauri::command]
fn back(app: tauri::AppHandle) {
    if let Some(content) = app.get_webview("content") {
        let _ = content.eval("history.back()");
    }
}

#[tauri::command]
fn forward(app: tauri::AppHandle) {
    if let Some(content) = app.get_webview("content") {
        let _ = content.eval("history.forward()");
    }
}

#[tauri::command]
fn reload(app: tauri::AppHandle) {
    if let Some(content) = app.get_webview("content") {
        let _ = content.reload();
    }
}

#[tauri::command]
fn navigate(app: tauri::AppHandle, url: String) {
    let url = app.state::<UrlMask>().unmask(&url);
    if let Some(content) = app.get_webview("content") {
        if let Ok(url) = Url::parse(&url) {
            let _ = content.navigate(url);
        }
    }
}

/// Enables two-finger swipe-to-navigate (back/forward) in the content webview.
#[cfg(target_os = "macos")]
fn enable_swipe_navigation(app: &tauri::AppHandle) {
    if let Some(content) = app.get_webview("content") {
        let _ = content.with_webview(|webview| unsafe {
            let view: &objc2_web_kit::WKWebView = &*webview.inner().cast();
            view.setAllowsBackForwardNavigationGestures(true);
        });
    }
}

/// Keeps the address bar in sync with the content webview's real URL.
fn start_url_poller(app: &tauri::AppHandle, mask: UrlMask) {
    let handle = app.clone();
    std::thread::spawn(move || {
        let mut last: Option<String> = None;
        loop {
            std::thread::sleep(std::time::Duration::from_millis(400));
            let Some(content) = handle.get_webview("content") else {
                continue;
            };
            let Ok(url) = content.url() else { continue };
            if url.scheme() != "http" && url.scheme() != "https" {
                continue;
            }
            if url.host_str() == Some("tauri.localhost") {
                continue;
            }
            let masked = mask.mask(url.as_str());
            if last.as_deref() != Some(masked.as_str()) {
                if let Some(toolbar) = handle.get_webview("main") {
                    let _ = toolbar.emit("url-changed", masked.clone());
                }
                last = Some(masked);
            }
        }
    });
}

/// Positions the toolbar strip at the top and the content webview below it.
fn layout(app: &tauri::AppHandle) {
    let Some(window) = app.get_window("main") else {
        eprintln!("quintodrome: layout: no \"main\" window");
        return;
    };
    let size = match window.inner_size() {
        Ok(size) => size,
        Err(err) => {
            eprintln!("quintodrome: layout: inner_size() failed: {err}");
            return;
        }
    };
    let scale = match window.scale_factor() {
        Ok(scale) => scale,
        Err(err) => {
            eprintln!("quintodrome: layout: scale_factor() failed: {err}");
            return;
        }
    };
    if size.width == 0 || size.height == 0 {
        eprintln!(
            "quintodrome: layout: window not realized yet ({}x{} physical), skipping",
            size.width, size.height
        );
        return;
    }
    let width = size.width as f64 / scale;
    let height = size.height as f64 / scale;

    if let Some(toolbar) = app.get_webview("main") {
        let _ = toolbar.set_auto_resize(false);
        let _ = toolbar.set_position(LogicalPosition::new(0.0, 0.0));
        let _ = toolbar.set_size(LogicalSize::new(width, TOOLBAR_HEIGHT));
    }
    if let Some(content) = app.get_webview("content") {
        let _ = content.set_auto_resize(false);
        let _ = content.set_position(LogicalPosition::new(0.0, TOOLBAR_HEIGHT));
        let _ = content.set_size(LogicalSize::new(width, (height - TOOLBAR_HEIGHT).max(0.0)));
    }
}

/// Installs a local NSEvent monitor that intercepts Cmd+X/C/V/A/Z and
/// dispatches the matching native editing selector to the first responder.
///
/// Menu key equivalents don't reach the webviews (content or devtools) in
/// Tauri on macOS, so we intercept the key events at the app level instead —
/// this is the only reliable way to make cut/copy/paste work in the devtools
/// console.
#[cfg(target_os = "macos")]
fn install_edit_shortcut_monitor(handle: &tauri::AppHandle) {
    use objc2::runtime::{AnyObject, Sel};
    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSApplication, NSEvent, NSEventMask, NSEventModifierFlags};
    use std::ptr::NonNull;

    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let app = NSApplication::sharedApplication(mtm);
    let handle = handle.clone();

    let block = block2::RcBlock::new(move |event: NonNull<NSEvent>| -> *mut NSEvent {
        let event = unsafe { event.as_ref() };
        let flags = event.modifierFlags();
        if !flags.contains(NSEventModifierFlags::Command) {
            return event as *const NSEvent as *mut NSEvent;
        }

        let key = event
            .charactersIgnoringModifiers()
            .map(|s| s.to_string().to_lowercase())
            .unwrap_or_default();

        let selector: Option<&'static std::ffi::CStr> = match key.as_str() {
            "x" => Some(c"cut:"),
            "c" => Some(c"copy:"),
            "v" => Some(c"paste:"),
            "a" => Some(c"selectAll:"),
            "z" => {
                if flags.contains(NSEventModifierFlags::Shift) {
                    Some(c"redo:")
                } else {
                    Some(c"undo:")
                }
            }
            _ => None,
        };

        let Some(selector) = selector else {
            return event as *const NSEvent as *mut NSEvent;
        };

        unsafe {
            let _: bool = app.sendAction_to_from(
                Sel::register(selector),
                None::<&AnyObject>,
                None::<&AnyObject>,
            );
        }

        // selectAll:/undo:/redo: don't dispatch through the responder chain in
        // WKWebView, so drive the content webview via execCommand too — but
        // only when an editable element is actually focused, otherwise we'd
        // select the whole document.
        let js = match key.as_str() {
            "a" => Some(
                "(function(){var e=document.activeElement;if(e&&(e.tagName==='INPUT'||e.tagName==='TEXTAREA'||e.isContentEditable))document.execCommand('selectAll')})()",
            ),
            "z" if flags.contains(NSEventModifierFlags::Shift) => {
                Some("document.execCommand('redo')")
            }
            "z" => Some("document.execCommand('undo')"),
            _ => None,
        };
        if let Some(js) = js {
            if let Some(content) = handle.get_webview("content") {
                let _ = content.eval(js);
            }
        }

        std::ptr::null_mut()
    });

    unsafe {
        let monitor =
            NSEvent::addLocalMonitorForEventsMatchingMask_handler(NSEventMask::KeyDown, &block);
        if let Some(monitor) = monitor {
            std::mem::forget(monitor);
        }
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_opener::init())
        // Native menu so Edit > Cut/Copy/Paste work via clicks.
        .menu(|handle| tauri::menu::Menu::default(handle))
        .manage(ServerState::default())
        .invoke_handler(tauri::generate_handler![back, forward, reload, navigate])
        .setup(|app| {
            let handle = app.handle().clone();

            let host = std::env::var("QUINTODROME_HOST")
                .unwrap_or_else(|_| server::DEFAULT_HOST.to_string());
            let port = std::env::var("QUINTODROME_PORT")
                .ok()
                .and_then(|p| p.parse::<u16>().ok())
                .unwrap_or(server::DEFAULT_PORT);

            let navidrome = Navidrome::new(host.clone(), port);
            let navidrome_url = navidrome.url.clone();

            let mask = UrlMask::new(&host, port);
            app.manage(mask.clone());

            // The primary webview ("main") is the toolbar; the Navidrome content
            // lives in a child webview below it.
            let content = WebviewBuilder::new("content", WebviewUrl::App("index.html".into()))
                .on_new_window({
                    let handle = handle.clone();
                    move |url, _features| {
                        // Open external links (Last.fm, etc.) in the default
                        // browser instead of a new in-app window.
                        if url.scheme() == "http" || url.scheme() == "https" {
                            let _ = handle.opener().open_url(url.to_string(), None::<&str>);
                        }
                        tauri::webview::NewWindowResponse::Deny
                    }
                });

            if let Some(window) = app.get_window("main") {
                // Best-effort initial sizing: the window's real geometry isn't
                // always known yet at this point (notably on Linux/Wayland),
                // so fall back to the configured window size rather than an
                // arbitrary guess. `layout()` below corrects this once the
                // window is realized.
                let (init_width, init_height) = window
                    .inner_size()
                    .ok()
                    .zip(window.scale_factor().ok())
                    .filter(|(size, _)| size.width > 0 && size.height > 0)
                    .map(|(size, scale)| (size.width as f64 / scale, size.height as f64 / scale))
                    .unwrap_or((DEFAULT_WIDTH, DEFAULT_HEIGHT));

                let _ = window.add_child(
                    content,
                    LogicalPosition::new(0.0, TOOLBAR_HEIGHT),
                    LogicalSize::new(init_width, (init_height - TOOLBAR_HEIGHT).max(0.0)),
                );

                // Never leave the toolbar in its default auto-resize state,
                // which fills the entire window - even if `layout()` below
                // can't yet compute a corrected size.
                if let Some(toolbar) = app.get_webview("main") {
                    let _ = toolbar.set_auto_resize(false);
                    let _ = toolbar.set_size(LogicalSize::new(init_width, TOOLBAR_HEIGHT));
                }

                let handle = handle.clone();
                window.on_window_event(move |event| {
                    if let WindowEvent::Resized(_) = event {
                        layout(&handle);
                    }
                });
            }

            layout(app.handle());

            // On Linux/Wayland the window's true geometry sometimes isn't
            // available yet during setup(), and no further Resized event
            // follows once it settles, so re-assert the layout shortly after
            // startup as a safety net.
            {
                let handle = handle.clone();
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_millis(250));
                    let _ = handle.run_on_main_thread(move || layout(&handle));
                });
            }

            #[cfg(target_os = "macos")]
            enable_swipe_navigation(app.handle());

            #[cfg(target_os = "macos")]
            install_edit_shortcut_monitor(app.handle());

            if let Some(toolbar) = app.get_webview("main") {
                let _ = toolbar.emit("url-changed", mask.mask(&navidrome_url));
            }

            start_url_poller(app.handle(), mask);

            // Detect/start Navidrome off the main thread, then load it.
            let content_url = navidrome_url.clone();
            std::thread::spawn(move || {
                if let Err(err) = server::ensure_running(&handle, &navidrome) {
                    eprintln!("quintodrome: {err}");
                    if let Some(content) = handle.get_webview("content") {
                        let message = serde_json::to_string(&err.to_string()).unwrap();
                        let _ = content.eval(&format!(
                            "document.getElementById('spinner').hidden = true;\
                             document.getElementById('status').hidden = true;\
                             var e = document.getElementById('error');\
                             e.hidden = false;\
                             e.textContent = 'Failed to start Navidrome: ' + {message};"
                        ));
                    }
                    return;
                }

                // Replace (not push) so the splash page isn't left in history —
                // otherwise "back" would return to the loading screen.
                if let Some(content) = handle.get_webview("content") {
                    let js = format!(
                        "location.replace({})",
                        serde_json::to_string(&content_url).unwrap()
                    );
                    let _ = content.eval(&js);
                }
            });

            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building Quintodrome")
        .run(|app, event| {
            if let RunEvent::Exit = event {
                server::shutdown(&app.state::<ServerState>());
            }
        });
}
