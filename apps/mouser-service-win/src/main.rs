//! `mouser-service.exe` (research V9): `MouserService` when the SCM starts
//! it, `--install` and `--uninstall` for the installer's hooks.

#[cfg(windows)]
fn main() -> std::process::ExitCode {
    use std::sync::Arc;

    use mouser_service_win::{args, machine::Machine, winapi_sys};

    let exe = std::env::current_exe().expect("the service knows where it is");
    let machine = Machine {
        api: Arc::new(winapi_sys::System),
        dir: exe.parent().expect("an executable is in a folder").into(),
        machine: r"C:\ProgramData\Mouser".into(),
        user: mouser_core::config::default_dir(),
        me: std::process::id(),
        host_name: std::env::var("COMPUTERNAME").unwrap_or_default(),
        store: mouser_platform_windows::WinPlatform::secret_store,
        scm: Box::new(winapi_sys::run_under_scm),
    };
    let args: Vec<String> = std::env::args().skip(1).collect();
    let code = args::run(&args, &machine, &mut std::io::stderr());
    std::process::ExitCode::from(code as u8)
}

/// Mouser has no service off Windows.
#[cfg(not(windows))]
fn main() {}
