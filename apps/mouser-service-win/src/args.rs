//! `mouser-service.exe`'s command line: `--install` and `--uninstall` for the
//! installer's hooks, nothing when the SCM starts it.

use std::io::Write;

/// What each command line runs; each gives the exit code.
pub trait Entries {
    fn install(&self) -> i32;
    fn uninstall(&self) -> i32;
    /// Run as `MouserService` under the SCM.
    fn service(&self) -> i32;
}

/// Exit code of a command line that is none of these.
pub const UNKNOWN: i32 = 3;

pub fn run(args: &[String], entries: &dyn Entries, stderr: &mut dyn Write) -> i32 {
    match args {
        [] => entries.service(),
        [one] if one == "--install" => entries.install(),
        [one] if one == "--uninstall" => entries.uninstall(),
        _ => {
            let _ = writeln!(
                stderr,
                "usage: mouser-service.exe [--install | --uninstall]"
            );
            UNKNOWN
        }
    }
}
