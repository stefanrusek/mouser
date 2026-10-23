// Release builds on Windows are GUI-subsystem programs: without this the exe is
// a console program and every launch opens a console window. Consequently the
// `--headless` refusal printed to stderr is not visible on Windows; the exit
// code (2) is the signal. Debug builds keep the console for development.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

// Returning the code, rather than `std::process::exit`, lets the process end
// normally, so exit-time work (such as writing coverage data) still runs.
#[cfg(feature = "gui")]
fn main() -> std::process::ExitCode {
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
        return std::process::ExitCode::SUCCESS;
    }
    if mouser_app::headless::is_headless(&args) {
        if let Err(refusal) =
            mouser_app::headless::check_supported(mouser_app::platform_select::current_os())
        {
            eprintln!("{refusal}");
            return std::process::ExitCode::from(2);
        }
        mouser_app::shell::run_headless();
    } else {
        mouser_app::shell::run();
    }
    std::process::ExitCode::SUCCESS
}

#[cfg(not(feature = "gui"))]
fn main() -> std::process::ExitCode {
    eprintln!(
        "This build of mouser-app has no window or tray; build it with --features gui to run Mouser."
    );
    std::process::ExitCode::from(2)
}
