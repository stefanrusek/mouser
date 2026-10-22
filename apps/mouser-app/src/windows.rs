//! The app's windows: first run (`main`), settings, the join prompt, the
//! arranger, the file-transfer window and the health panel.

use mouser_proto::control::{ConnectionState, Method, Status};
use mouser_proto::model::PermissionState;
use tauri::{AppHandle, Manager, Runtime, WebviewUrl, WebviewWindowBuilder};

use mouser_core::ControlHandle;

/// Which window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Which {
    FirstRun,
    Settings,
    Join,
    Arranger,
    Transfer,
    Health,
}

impl Which {
    pub fn label(self) -> &'static str {
        match self {
            Self::FirstRun => "main",
            Self::Settings => "settings",
            Self::Join => "join",
            Self::Arranger => "arranger",
            Self::Transfer => "transfer",
            Self::Health => "health",
        }
    }

    pub(crate) fn page(self) -> &'static str {
        match self {
            Self::FirstRun => "first_run.html",
            Self::Settings => "desk_settings.html",
            Self::Join => "join_prompt.html",
            Self::Arranger => "arranger.html",
            Self::Transfer => "transfer.html",
            Self::Health => "health.html",
        }
    }

    pub(crate) fn title(self) -> &'static str {
        match self {
            Self::FirstRun => "Welcome to Mouser",
            Self::Settings => "Mouser",
            Self::Join => "A computer wants to join",
            Self::Arranger => "Arrange displays",
            Self::Transfer => "File transfer",
            Self::Health => "Mouser health",
        }
    }

    pub(crate) fn size(self) -> (f64, f64) {
        match self {
            Self::FirstRun => (560.0, 640.0),
            Self::Settings => (620.0, 720.0),
            Self::Join => (460.0, 320.0),
            Self::Arranger => (700.0, 760.0),
            Self::Transfer => (460.0, 360.0),
            Self::Health => (560.0, 640.0),
        }
    }
}

/// Show and focus the window, creating it first if it was closed.
pub fn show<R: Runtime>(app: &AppHandle<R>, which: Which) {
    let window = match app.get_webview_window(which.label()) {
        Some(window) => window,
        None => {
            let (width, height) = which.size();
            crate::portable::with_webview_data(
                app,
                WebviewWindowBuilder::new(app, which.label(), WebviewUrl::App(which.page().into())),
            )
            .title(which.title())
            .inner_size(width, height)
            .visible(false)
            .build()
            .expect("window can be created")
        }
    };
    window.show().expect("window can be shown");
    window.set_focus().expect("window can be focused");
}

/// Whether the window exists (it is created when needed and may be closed).
pub fn is_open<R: Runtime>(app: &AppHandle<R>, which: Which) -> bool {
    app.get_webview_window(which.label()).is_some()
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
/// desk settings after that.
pub async fn open_home<R: Runtime>(app: &AppHandle<R>, control: &ControlHandle) {
    let status = control
        .call(Method::Status, None)
        .await
        .expect("status is available");
    let status: Status = serde_json::from_value(status).expect("status matches its schema");
    show(
        app,
        if needs_setup(&status) {
            Which::FirstRun
        } else {
            Which::Settings
        },
    );
}
