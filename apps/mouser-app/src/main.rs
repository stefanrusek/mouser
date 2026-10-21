#[cfg(feature = "gui")]
fn main() {
    mouser_app::shell::run();
}

// Returning the code, rather than `std::process::exit`, lets the process end
// normally, so exit-time work (such as writing coverage data) still runs.
#[cfg(not(feature = "gui"))]
fn main() -> std::process::ExitCode {
    eprintln!(
        "This build of mouser-app has no window or tray; build it with --features gui to run Mouser."
    );
    std::process::ExitCode::from(2)
}
