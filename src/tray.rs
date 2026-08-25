//! Status icon, so the app survives its own window.
//!
//! Closing the main window only calls `remove_window()` -- the daemon and its
//! hotkey keep running -- but until now that left nothing on screen to bring
//! it back or shut it down, and the only way back in was to launch the binary
//! again. This publishes a StatusNotifierItem, which is what GNOME's
//! appindicator extension (and every other tray host) consumes.
//!
//! The item owns no state of its own: clicking it or its menu just posts the
//! same `EngineEvent`s the D-Bus `Show` method already posts, so the tray, a
//! second launch, and the window itself all go through one path.

use crate::state::EngineEvent;
use crossbeam_channel::Sender;
use ksni::blocking::{Handle, TrayMethods};
use ksni::menu::StandardItem;
use ksni::{Category, MenuItem, Status, Tray};

pub struct DictationTray {
    tx: Sender<EngineEvent>,
    /// Directory prepended to the host's icon search path. Set when running
    /// from a checkout, so the icon resolves before `install.sh` has put it
    /// in the user's hicolor theme; empty once installed, where the theme
    /// lookup finds it on its own.
    icon_theme_path: String,
}

impl Tray for DictationTray {
    fn id(&self) -> String {
        "dictation-tool".into()
    }

    fn title(&self) -> String {
        "Dictation".into()
    }

    fn category(&self) -> Category {
        Category::ApplicationStatus
    }

    fn status(&self) -> Status {
        Status::Active
    }

    fn icon_name(&self) -> String {
        "dictation-tool".into()
    }

    fn icon_theme_path(&self) -> String {
        self.icon_theme_path.clone()
    }

    /// Left click. Mirrors what a second launch of the binary does.
    fn activate(&mut self, _x: i32, _y: i32) {
        let _ = self.tx.send(EngineEvent::ShowWindow);
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        vec![
            StandardItem {
                label: "Show window".into(),
                activate: Box::new(|tray: &mut Self| {
                    let _ = tray.tx.send(EngineEvent::ShowWindow);
                }),
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            StandardItem {
                label: "Quit".into(),
                activate: Box::new(|tray: &mut Self| {
                    let _ = tray.tx.send(EngineEvent::Quit);
                }),
                ..Default::default()
            }
            .into(),
        ]
    }
}

/// Publishes the status icon. The returned handle must be kept alive for as
/// long as the icon should exist; dropping it withdraws the item.
///
/// A missing tray host is not an error worth stopping for -- the hotkey is
/// the point of the app and works without it -- so failure is reported and
/// the daemon carries on windowless-but-working, exactly as before.
pub fn spawn(tx: Sender<EngineEvent>, icon_theme_path: String) -> Option<Handle<DictationTray>> {
    match (DictationTray { tx, icon_theme_path }).spawn() {
        Ok(handle) => Some(handle),
        Err(err) => {
            eprintln!("[tray] no status icon ({err}); the hotkey still works");
            None
        }
    }
}
