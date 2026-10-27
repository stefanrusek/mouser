//! The live tray icon: its menu, its click handling and how a [`TrayView`]
//! is drawn on it. Runs only inside a launched app; the decisions it makes
//! are in `tray.rs` and are unit tested there.

use tauri::image::Image;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{TrayIcon, TrayIconBuilder};
use tauri::{AppHandle, Runtime};

use mouser_core::ControlHandle;

use crate::shell::LiveNotifier;
use crate::tray::{self, TrayPort, TrayView};
use crate::{exit, updater};

pub struct LiveTray<R: Runtime> {
    pub app: AppHandle<R>,
    pub icon: TrayIcon<R>,
}

impl<R: Runtime> TrayPort for LiveTray<R> {
    fn apply(&self, view: &TrayView) {
        let items: Vec<MenuItem<R>> = view
            .entries
            .iter()
            .map(|e| {
                MenuItem::with_id(
                    &self.app,
                    e.id.unwrap_or("summary"),
                    &e.label,
                    e.id.is_some(),
                    None::<&str>,
                )
                .expect("menu item can be created")
            })
            .collect();
        let refs: Vec<&dyn tauri::menu::IsMenuItem<R>> = items.iter().map(|i| i as _).collect();
        let menu = Menu::with_items(&self.app, &refs).expect("menu can be created");
        let image = Image::from_bytes(view.icon.png()).expect("tray icon is a PNG");
        self.icon
            .set_icon(Some(image))
            .expect("tray icon can be set");
        self.icon
            .set_icon_as_template(view.icon.is_template())
            .expect("tray template flag can be set");
        self.icon
            .set_tooltip(Some(&view.tooltip))
            .expect("tray tooltip can be set");
        self.icon
            .set_menu(Some(menu))
            .expect("tray menu can be set");
    }
}

/// Build the tray icon: the menu opens on the right button, and the left
/// button pauses or resumes.
pub fn build<R: Runtime>(
    app: &tauri::App<R>,
    control: ControlHandle,
) -> tauri::Result<TrayIcon<R>> {
    let click_control = control.clone();
    TrayIconBuilder::with_id("mouser")
        .show_menu_on_left_click(false)
        .on_tray_icon_event(move |_icon, event| tray::handle_tray_event(&click_control, &event))
        .on_menu_event(move |app, event| {
            tray::handle_menu(
                app,
                &control,
                event.id.as_ref(),
                &mut |action| match action {
                    tray::Action::SendCad => {
                        let (app, control) = (app.clone(), control.clone());
                        tauri::async_runtime::spawn(async move {
                            let result = crate::tray_sas::send(&control).await;
                            let live = LiveNotifier(app);
                            crate::tray_sas::report(
                                result,
                                crate::notify::gated(&control, &live).await,
                            );
                        });
                    }
                    tray::Action::Update => {
                        let control = control.clone();
                        tauri::async_runtime::spawn(async move { updater::apply(&control).await });
                    }
                    _ => exit::quit(app, || app.exit(0)),
                },
            )
        })
        .build(app)
}
