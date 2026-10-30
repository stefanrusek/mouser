use crate::shell::*;
use std::time::{Duration, Instant};
use windows::Win32::Foundation::{ERROR_ACCESS_DENIED, ERROR_CANCELLED};

#[test]
fn a_prompt_the_user_declined_is_declined_and_anything_else_a_failure() {
    let declined = windows::core::Error::from(ERROR_CANCELLED.to_hresult());
    assert_eq!(outcome(&declined), RunAs::Declined);
    let refused = windows::core::Error::from(ERROR_ACCESS_DENIED.to_hresult());
    let RunAs::Failed(why) = outcome(&refused) else {
        panic!("refused is a failure");
    };
    assert!(!why.is_empty());
}

#[test]
fn a_program_is_started_through_the_shell_with_its_arguments() {
    // The verb `open`, never `runas`: tests do not elevate.
    let flag = std::env::temp_dir().join(format!("mouser-shell-test-{}", std::process::id()));
    let _ = std::fs::remove_file(&flag);
    let cmd = std::path::Path::new(r"C:\Windows\System32\cmd.exe");
    let args = format!("/c type nul > \"{}\"", flag.display());
    assert_eq!(start(cmd, &args, false), Ok(()));
    // The started program ends by itself once it has written its file.
    let end = Instant::now() + Duration::from_secs(10);
    while !flag.exists() && Instant::now() < end {
        std::thread::sleep(Duration::from_millis(20));
    }
    let made = flag.exists();
    let _ = std::fs::remove_file(&flag);
    assert!(made, "the program ran with its arguments");
}

#[test]
fn a_program_that_is_not_there_is_a_failure() {
    let missing = std::path::Path::new(r"C:\no\such\mouser.exe");
    assert!(matches!(
        shell_execute("open", missing, ""),
        Err(RunAs::Failed(_))
    ));
}

#[test]
fn a_process_is_alive_until_it_ends_and_its_end_can_be_waited_for() {
    let mut ping = std::process::Command::new("ping")
        .args(["-n", "2", "127.0.0.1"])
        .stdout(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let pid = ping.id();
    assert!(process_alive(pid));
    wait_for_process(pid);
    assert!(!process_alive(pid), "ended");
    ping.wait().unwrap();
    // Process ids are multiples of four: this one names no process.
    assert!(!process_alive(u32::MAX));
    let started = Instant::now();
    wait_for_process(u32::MAX);
    assert!(started.elapsed() < Duration::from_secs(1));
}

#[test]
fn this_process_is_elevated_as_its_integrity_level_says() {
    let level = crate::uipi::integrity_level(std::process::id());
    assert_eq!(this_process_elevated(), crate::foreground::is_admin(level));
}

#[test]
fn a_message_box_waits_until_it_is_closed() {
    use windows::Win32::Foundation::{LPARAM, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{FindWindowW, PostMessageW, WM_CLOSE};
    use windows::core::w;
    let closer = std::thread::spawn(|| {
        let end = Instant::now() + Duration::from_secs(10);
        while Instant::now() < end {
            if let Ok(dialog) = unsafe { FindWindowW(w!("#32770"), w!("Mouser")) } {
                unsafe { PostMessageW(Some(dialog), WM_CLOSE, WPARAM(0), LPARAM(0)) }.unwrap();
                return true;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        false
    });
    message_box("A test of the message box.");
    assert!(closer.join().unwrap(), "the box was shown and closed");
}

#[test]
fn a_running_service_is_running_and_one_that_is_not_there_is_not() {
    // The Windows Event Log runs on every Windows.
    assert!(service_running("EventLog"));
    assert!(!service_running("NoSuchMouserServiceForTests"));
}

#[test]
fn a_process_run_by_a_user_is_not_local_system() {
    assert_eq!(LOCAL_SYSTEM, "S-1-5-18");
    assert_eq!(
        this_process_is_local_system(),
        crate::secrets::current_user_sid() == LOCAL_SYSTEM
    );
    assert!(!this_process_is_local_system());
}
