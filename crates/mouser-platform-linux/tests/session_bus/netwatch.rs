//! `watch_network` follows logind's resume signal on the system bus.

use crate::daemon::{Daemon, FAKES};
use mouser_platform::Platform;
use mouser_platform_linux::{LinuxPlatform, Settings};
use std::collections::HashMap;
use std::sync::mpsc::{RecvTimeoutError, channel};
use std::time::Duration;

fn on_system_bus(bus: &Daemon) -> LinuxPlatform {
    LinuxPlatform::with_settings(Settings {
        env: HashMap::from([("DBUS_SYSTEM_BUS_ADDRESS".to_string(), bus.address.clone())]),
        sys_class_net: "/sys/class/net".into(),
        os_release: "/etc/os-release".into(),
        exe: "/bin/mouser".into(),
        device_wait: Duration::from_millis(300),
    })
}

fn logind(bus: &Daemon) -> zbus::Connection {
    FAKES
        .block_on(
            zbus::connection::Builder::address(bus.address.as_str())
                .unwrap()
                .name("org.freedesktop.login1")
                .unwrap()
                .build(),
        )
        .unwrap()
}

fn prepare_for_sleep(conn: &zbus::Connection, sleeping: bool) {
    FAKES
        .block_on(conn.emit_signal(
            None::<&str>,
            "/org/freedesktop/login1",
            "org.freedesktop.login1.Manager",
            "PrepareForSleep",
            &(sleeping,),
        ))
        .unwrap();
}

#[test]
fn waking_from_sleep_fires_the_network_watch_and_going_to_sleep_does_not() {
    let bus = Daemon::start();
    let login = logind(&bus);
    let platform = on_system_bus(&bus);
    let (tx, rx) = channel();
    let handle = platform.watch_network(tx).unwrap();
    prepare_for_sleep(&login, true);
    assert_eq!(
        rx.recv_timeout(Duration::from_millis(300)),
        Err(RecvTimeoutError::Timeout)
    );
    prepare_for_sleep(&login, false);
    rx.recv_timeout(Duration::from_secs(10)).unwrap();
    drop(handle);
    prepare_for_sleep(&login, false);
    assert_eq!(
        rx.recv_timeout(Duration::from_millis(300)),
        Err(RecvTimeoutError::Disconnected)
    );
}

#[test]
fn the_network_watch_survives_the_system_bus_going_away() {
    let bus = Daemon::start();
    let platform = on_system_bus(&bus);
    let (tx, rx) = channel();
    let handle = platform.watch_network(tx).unwrap();
    drop(bus);
    // The resume stream ends with the bus; the watch itself stays up until
    // its handle is dropped.
    std::thread::sleep(Duration::from_millis(500));
    assert_eq!(
        rx.recv_timeout(Duration::from_millis(100)),
        Err(RecvTimeoutError::Timeout)
    );
    drop(handle);
    assert_eq!(rx.recv(), Err(std::sync::mpsc::RecvError));
}
