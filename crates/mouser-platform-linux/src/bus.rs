//! The session bus and the one runtime every portal call runs on.

use std::collections::HashMap;
use std::future::Future;
use std::sync::OnceLock;
use tokio::runtime::{Builder, Runtime};
use zbus::Connection;

fn runtime() -> &'static Runtime {
    static RUNTIME: OnceLock<Runtime> = OnceLock::new();
    RUNTIME.get_or_init(|| {
        Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .expect("portal runtime")
    })
}

/// Runs `f` on the portal runtime and waits for it from any thread, inside
/// or outside another runtime. A panic in `f` panics the caller.
pub(crate) fn block_on<T: Send + 'static>(f: impl Future<Output = T> + Send + 'static) -> T {
    let (tx, rx) = std::sync::mpsc::channel();
    runtime().spawn(async move {
        tx.send(f.await).expect("caller waiting");
    });
    rx.recv().expect("portal task panicked")
}

/// Starts `f` on the portal runtime without waiting for it.
pub(crate) fn spawn(f: impl Future<Output = ()> + Send + 'static) -> tokio::task::JoinHandle<()> {
    runtime().spawn(f)
}

/// The session bus address: `DBUS_SESSION_BUS_ADDRESS`, else the systemd
/// default under `XDG_RUNTIME_DIR`.
pub(crate) fn address(env: &HashMap<String, String>) -> Option<String> {
    env.get("DBUS_SESSION_BUS_ADDRESS").cloned().or_else(|| {
        env.get("XDG_RUNTIME_DIR")
            .map(|dir| format!("unix:path={dir}/bus"))
    })
}

pub(crate) async fn connect(env: &HashMap<String, String>) -> zbus::Result<Connection> {
    let address = address(env).ok_or_else(|| zbus::Error::Address("no session bus".into()))?;
    zbus::connection::Builder::address(address.as_str())?
        .build()
        .await
}

/// The system bus address: `DBUS_SYSTEM_BUS_ADDRESS`, else the standard socket.
pub(crate) fn system_address(env: &HashMap<String, String>) -> String {
    env.get("DBUS_SYSTEM_BUS_ADDRESS")
        .cloned()
        .unwrap_or_else(|| "unix:path=/var/run/dbus/system_bus_socket".into())
}

pub(crate) async fn connect_system(env: &HashMap<String, String>) -> zbus::Result<Connection> {
    zbus::connection::Builder::address(system_address(env).as_str())?
        .build()
        .await
}
