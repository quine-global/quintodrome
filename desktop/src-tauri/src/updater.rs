//! Updates through Sparkle, the standard macOS updater
//! (tauri-plugin-sparkle-updater): Quintodrome › Check for Updates… in the
//! app menu, plus Sparkle's own daily background check. Sparkle shows the
//! native "A new version is available" window and installs the update itself.
//!
//! Settings live in Info.plist (feed URL, public key, automatic checks). The
//! release workflow writes one appcast per architecture into the Homebrew tap;
//! Info.plist names the arm64 one and Intel builds switch to theirs here.

use tauri::menu::{Menu, MenuItem, Submenu};
use tauri::{AppHandle, Runtime};
use tauri_plugin_sparkle_updater::SparkleUpdaterExt;

pub const CHECK_FOR_UPDATES_ID: &str = "check-for-updates";

#[cfg(target_arch = "x86_64")]
const INTEL_FEED_URL: &str = "https://git.quineglobal.com/quine-global/homebrew-tap.git/tree/appcasts/quintodrome-x86_64.xml?raw=true";

/// Points Intel builds at their own appcast.
pub fn setup<R: Runtime>(app: &AppHandle<R>) {
    #[cfg(target_arch = "x86_64")]
    if let Some(updater) = app.sparkle_updater() {
        if let Err(err) = updater.set_feed_url_override(Some(INTEL_FEED_URL.to_string())) {
            eprintln!("quintodrome: couldn't set the Intel appcast: {err}");
        }
    }
    #[cfg(not(target_arch = "x86_64"))]
    let _ = app;
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
/// window either way ("You're up to date" included).
pub fn check_for_updates<R: Runtime>(app: &AppHandle<R>) {
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
