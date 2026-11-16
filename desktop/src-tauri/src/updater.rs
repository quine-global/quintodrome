//! Updates through Sparkle, the standard macOS updater
//! (tauri-plugin-sparkle-updater): Quintodrome › Check for Updates… in the
//! app menu, a background check at every launch, plus Sparkle's own daily one. Sparkle shows the
//! native "A new version is available" window and installs the update itself.
//!
//! An update Sparkle has already downloaded silently (the user ticked
//! "Automatically download and install updates") waits to install when the app
//! quits; Sparkle's own check shows nothing then, so Check for Updates… says
//! so itself and offers to quit.
//!
//! Settings live in Info.plist (feed URL, public key, automatic checks). The
//! release workflow writes one appcast per architecture into the Homebrew tap;
//! Info.plist names the arm64 one and Intel builds switch to theirs here.

use std::sync::{Arc, Mutex};

use objc2::MainThreadMarker;
use objc2_app_kit::{NSAlert, NSAlertFirstButtonReturn};
use objc2_foundation::NSString;
use tauri::menu::{Menu, MenuItem, Submenu};
use tauri::{AppHandle, Runtime};
use tauri_plugin_sparkle_updater::SparkleUpdaterExt;

/// The version Sparkle has downloaded and will install when the app quits.
static QUEUED_UPDATE: Mutex<Option<String>> = Mutex::new(None);

pub const CHECK_FOR_UPDATES_ID: &str = "check-for-updates";

#[cfg(target_arch = "x86_64")]
const INTEL_FEED_URL: &str = "https://git.quineglobal.com/quine-global/homebrew-tap.git/tree/appcasts/quintodrome-x86_64.xml?raw=true";

/// Points Intel builds at their own appcast, and notes an update Sparkle
/// queues to install on quit.
pub fn setup<R: Runtime>(app: &AppHandle<R>) {
    if let Some(updater) = app.sparkle_updater() {
        updater.set_event_callback(Some(Arc::new(|name, payload| {
            if name == "sparkle://will-install-update-on-quit" {
                let version = payload
                    .get("version")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string();
                *QUEUED_UPDATE.lock().unwrap() = Some(version);
            }
        })));
    }
    #[cfg(target_arch = "x86_64")]
    if let Some(updater) = app.sparkle_updater() {
        if let Err(err) = updater.set_feed_url_override(Some(INTEL_FEED_URL.to_string())) {
            eprintln!("quintodrome: couldn't set the Intel appcast: {err}");
        }
    }

    // Sparkle's schedule checks once a day; check at every launch as well,
    // in the background (only an update shows anything).
    if let Some(updater) = app.sparkle_updater() {
        if updater.automatically_checks_for_updates().unwrap_or(false) {
            if let Err(err) = updater.check_for_updates_in_background() {
                eprintln!("quintodrome: launch update check failed: {err}");
            }
        }
    }
}

/// The default macOS menu, with "Check for Updates…" under About in the app
/// menu, where Mac apps put it.
pub fn menu<R: Runtime>(handle: &AppHandle<R>) -> tauri::Result<Menu<R>> {
    let menu = Menu::default(handle)?;
    let app_menu = menu
        .items()?
        .into_iter()
        .next()
        .and_then(|item| item.as_submenu().cloned());
    if let Some(app_menu) = app_menu {
        insert_check_for_updates(handle, &app_menu)?;
    }
    Ok(menu)
}

fn insert_check_for_updates<R: Runtime>(
    handle: &AppHandle<R>,
    app_menu: &Submenu<R>,
) -> tauri::Result<()> {
    let item = MenuItem::with_id(
        handle,
        CHECK_FOR_UPDATES_ID,
        "Check for Updates…",
        true,
        None::<&str>,
    )?;
    // Right after About (position 0), before the separator that follows it.
    app_menu.insert(&item, 1)?;
    Ok(())
}

/// Handles the menu item: Sparkle's user-initiated check, which shows its
/// window either way ("You're up to date" included), unless an update is
/// already queued for quit, which gets its own message.
pub fn check_for_updates<R: Runtime>(app: &AppHandle<R>) {
    let queued = QUEUED_UPDATE.lock().unwrap().clone();
    if let Some(version) = queued {
        if update_queued_alert(&version) {
            app.exit(0);
        }
        return;
    }
    match app.sparkle_updater() {
        Some(updater) => {
            if let Err(err) = updater.check_for_updates() {
                eprintln!("quintodrome: update check failed: {err}");
            }
        }
        // Not a .app bundle (e.g. `tauri dev`): Sparkle doesn't run.
        None => eprintln!("quintodrome: updates are only available in the packaged app"),
    }
}

/// "Already downloaded, installs when you quit": returns true for Quit Now.
/// Runs on the main thread (menu events do).
fn update_queued_alert(version: &str) -> bool {
    let Some(mtm) = MainThreadMarker::new() else {
        return false;
    };
    let title = if version.is_empty() {
        "An update is ready to install".to_string()
    } else {
        format!("Quintodrome {version} is ready to install")
    };
    let alert = NSAlert::new(mtm);
    alert.setMessageText(&NSString::from_str(&title));
    alert.setInformativeText(&NSString::from_str(
        "It's already downloaded and installs when you quit Quintodrome. Open the app again afterwards to use the new version.",
    ));
    alert.addButtonWithTitle(&NSString::from_str("Quit Now"));
    alert.addButtonWithTitle(&NSString::from_str("Later"));
    alert.runModal() == NSAlertFirstButtonReturn
}
