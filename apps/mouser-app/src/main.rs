// Release builds on Windows are GUI-subsystem programs: without this the exe is
// a console program and every launch opens a console window. Debug builds keep
// the console for development.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

// Returning the code, rather than `std::process::exit`, lets the process end
// normally, so exit-time work (such as writing coverage data) still runs.
/// `--cursor-guard <pid>` is handled before anything else starts: no Tauri,
/// no window.
fn guard_first(args: &[String]) -> Option<std::process::ExitCode> {
    let os = mouser_app::platform_select::current_os();
    mouser_app::args::first(args, os, mouser_app::args::GUARD)
}

/// On Windows, before Tauri: waits for the instance a `--wait-for <pid>`
/// launch replaces, and ends a second launch that is not elevated while an
/// elevated portable instance runs (FR-005; probe P3). Whether to end here.
#[cfg(target_os = "windows")]
fn ends_before_tauri(args: &[String]) -> bool {
    let paths = mouser_core::paths::Paths::resolve(
        &std::env::current_exe().expect("the executable has a path"),
        mouser_app::platform_select::current_os(),
        &mouser_core::paths::Env::from_process(),
    );
    let elevated = mouser_platform_windows::shell::this_process_elevated();
    mouser_app::elevation::before_tauri(args, &paths, elevated, &mouser_app::elevation::System)
}

#[cfg(not(target_os = "windows"))]
fn ends_before_tauri(_args: &[String]) -> bool {
    false
}

/// What a launch does before anything else; the exit code when it ends there.
fn first_steps(args: &[String]) -> Option<std::process::ExitCode> {
    guard_first(args).or_else(|| ends_before_tauri(args).then_some(std::process::ExitCode::SUCCESS))
}

#[cfg(feature = "gui")]
fn main() -> std::process::ExitCode {
    let args: Vec<String> = std::env::args().collect();
    if let Some(code) = first_steps(&args) {
        return code;
    }
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
        mouser_app::shell::run_headless();
    } else {
        mouser_app::shell::run();
    }
    std::process::ExitCode::SUCCESS
}

#[cfg(not(feature = "gui"))]
fn main() -> std::process::ExitCode {
    let args: Vec<String> = std::env::args().collect();
    if let Some(code) = first_steps(&args) {
        return code;
    }
    eprintln!(
        "This build of mouser-app has no window or tray; build it with --features gui to run Mouser."
    );
    std::process::ExitCode::from(2)
}
