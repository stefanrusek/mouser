#[cfg(feature = "gui")]
fn main() {
    let args: Vec<String> = std::env::args().collect();
    #[cfg(target_os = "macos")]
    if let Some(staging) = mouser_app::login_screen::install_arg(&args) {
        // Run as root by the "Allow control at the login screen" prompt.
        let exe = std::env::current_exe().expect("the executable has a path");
        mouser_app::login_screen::install_as_root(
            &staging,
            &mouser_app::headless::machine_dir(mouser_platform::Os::MacOs),
            &mouser_platform_macos::login_agent::LoginAgent::system(exe),
        );
        return;
    }
    if mouser_app::headless::is_headless(&args) {
        mouser_app::shell::run_headless();
    } else {
        mouser_app::shell::run();
    }
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
