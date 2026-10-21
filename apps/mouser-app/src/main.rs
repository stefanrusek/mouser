#[cfg(feature = "gui")]
fn main() {
    mouser_app::shell::run();
}

#[cfg(not(feature = "gui"))]
fn main() {
    eprintln!(
        "This build of mouser-app has no window or tray; build it with --features gui to run Mouser."
    );
    std::process::exit(2);
}
