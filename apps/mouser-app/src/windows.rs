//! The app's one normal window, `main`, and how everything navigates it
//! (contracts/ui-navigation.md).

use mouser_proto::control::{ConnectionState, Method, Status};
use mouser_proto::model::PermissionState;
#[cfg(feature = "gui")]
use tauri::WindowEvent;
use tauri::{AppHandle, Emitter, Manager, Runtime, WebviewUrl, WebviewWindowBuilder};

use crate::host::Control;

/// The one normal window.
pub const LABEL: &str = "main";

/// The event the window listens to; the payload is `{route}`.
pub const EVENT: &str = "mouser://navigate";

pub const DESK: &str = "#desk";
pub const DISPLAYS: &str = "#displays";
pub const HEALTH: &str = "#health";
pub const WELCOME: &str = "#welcome";

/// Mouser has a Dock icon only while its window is shown (macOS).
#[cfg(target_os = "macos")]
pub fn activation_policy(window_visible: bool) -> tauri::ActivationPolicy {
    if window_visible {
        tauri::ActivationPolicy::Regular
    } else {
        tauri::ActivationPolicy::Accessory
    }
}

#[cfg(target_os = "macos")]
fn follow_window<R: Runtime>(app: &AppHandle<R>, window_visible: bool) {
    app.set_activation_policy(activation_policy(window_visible))
        .expect("the activation policy can be set");
}

#[cfg(not(target_os = "macos"))]
fn follow_window<R: Runtime>(_app: &AppHandle<R>, _window_visible: bool) {}

fn create<R: Runtime>(app: &AppHandle<R>, route: &str) -> tauri::WebviewWindow<R> {
    crate::portable::with_webview_data(
        app,
        WebviewWindowBuilder::new(
            app,
            LABEL,
            WebviewUrl::App(format!("index.html{route}").into()),
        ),
    )
    .title("Mouser")
    .inner_size(960.0, 680.0)
    .min_inner_size(720.0, 520.0)
    .resizable(true)
    .visible(false)
    .build()
    .expect("window can be created")
}

/// Closing the window hides it (it is reused on the next open) and takes the
/// Dock icon away. Installed by the live shell, which receives window events.
#[cfg(feature = "gui")]
pub fn hide_on_close<R: Runtime>(window: &tauri::Window<R>, event: &WindowEvent) {
    if let WindowEvent::CloseRequested { api, .. } = event
        && window.label() == LABEL
    {
        api.prevent_close();
        window.hide().expect("window can be hidden");
        follow_window(window.app_handle(), false);
    }
}

/// Bring the one window to the front at `route` (a location hash such as
/// `#displays`), creating it first if it does not exist.
pub fn navigate<R: Runtime>(app: &AppHandle<R>, route: &str) {
    let window = match app.get_webview_window(LABEL) {
        Some(window) => {
            window.unminimize().expect("window can be restored");
            window
                .emit(EVENT, serde_json::json!({ "route": route }))
                .expect("the route can be announced");
            window
        }
        None => create(app, route),
    };
    follow_window(app, true);
    window.show().expect("window can be shown");
    window.set_focus().expect("window can be focused");
}

/// Whether the window is on screen.
pub fn is_shown<R: Runtime>(app: &AppHandle<R>) -> bool {
    app.get_webview_window(LABEL)
        .is_some_and(|w| w.is_visible().expect("visibility is readable"))
}

/// First-run is not finished while this computer is off a desk or is missing
/// an operating-system permission.
pub fn needs_setup(status: &Status) -> bool {
    status.desk.is_none()
        || status.computers.iter().any(|c| {
            c.connection == ConnectionState::SelfComputer
                && c.permissions
                    .values()
                    .any(|s| *s == PermissionState::Missing)
        })
}

/// Open the window the owner wants now: setup while it is unfinished, the
/// desk after that.
pub async fn open_home<R: Runtime>(app: &AppHandle<R>, control: &Control) {
    let status = control
        .call(Method::Status, None)
        .await
        .expect("status is available");
    let status: Status = serde_json::from_value(status).expect("status matches its schema");
    navigate(app, if needs_setup(&status) { WELCOME } else { DESK });
}
