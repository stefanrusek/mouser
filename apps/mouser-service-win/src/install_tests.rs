use std::path::{Path, PathBuf};

use crate::fake::{Call, FakeWin};
use crate::install::{exit_code, install, uninstall};
use crate::winapi::{Account, FirewallRule, ServiceSpec, StartType};

const DIR: &str = r"C:\Program Files\Mouser";
const POLICY: &str = r"SOFTWARE\Microsoft\Windows\CurrentVersion\Policies\System";
/// The uninstaller's own process (`mouser-service.exe --uninstall`).
const ME: u32 = 77;

fn dir() -> PathBuf {
    PathBuf::from(DIR)
}

fn spec() -> ServiceSpec {
    ServiceSpec {
        name: "MouserService".into(),
        display_name: "Mouser".into(),
        executable: Path::new(DIR).join("mouser-service.exe"),
        account: Account::LocalSystem,
        start: StartType::Automatic,
    }
}

fn rule() -> FirewallRule {
    FirewallRule {
        name: "Mouser".into(),
        program: Path::new(DIR).join("mouser-app.exe"),
        udp_port: 47801,
    }
}

/// What `--install` or `--uninstall` exits with, and what it wrote to stderr.
fn run(result: Result<(), String>) -> (i32, String) {
    let mut stderr = Vec::new();
    let code = exit_code(result, &mut stderr);
    (code, String::from_utf8(stderr).unwrap())
}

/// A computer Mouser is installed on, with its service running.
fn installed() -> FakeWin {
    let fake = FakeWin::new();
    install_on(&fake).unwrap();
    fake.start_process("mouser-service.exe", 0);
    fake
}

#[test]
fn install_creates_mouser_service_as_local_system_started_automatically() {
    let fake = FakeWin::new();
    install_on(&fake).unwrap();
    assert_eq!(fake.calls()[0], Call::CreateService(spec()));
}

#[test]
fn install_lets_services_send_ctrl_alt_del() {
    let fake = FakeWin::new();
    install_on(&fake).unwrap();
    assert_eq!(fake.dword(POLICY, "SoftwareSASGeneration"), Some(1));
}

#[test]
fn install_lets_desk_mates_reach_mouser_app_on_udp_47801() {
    let fake = FakeWin::new();
    install_on(&fake).unwrap();
    assert_eq!(fake.rules(), [rule()]);
}

#[test]
fn install_starts_the_service_last_and_exits_0() {
    let fake = FakeWin::new();
    assert_eq!(run(install_on(&fake)), (0, String::new()));
    assert_eq!(
        fake.calls().last(),
        Some(&Call::StartService("MouserService".into()))
    );
    assert_eq!(fake.service("MouserService"), Some((spec(), true)));
}

#[test]
fn install_over_an_installed_mouser_keeps_what_is_there() {
    let fake = installed();
    // The service exists and runs; the rule is there.
    fake.fail("create_service", 1073);
    fake.fail("start_service", 1056);
    assert_eq!(run(install_on(&fake)), (0, String::new()));
    assert_eq!(fake.rules(), [rule()]);
}

#[test]
fn a_step_of_install_that_fails_exits_1_with_its_reason() {
    for (method, reason) in [
        (
            "create_service",
            "creating MouserService: create_service failed (error 5)",
        ),
        (
            "write_dword",
            "setting SoftwareSASGeneration: write_dword failed (error 5)",
        ),
        (
            "has_firewall_rule",
            "adding the firewall rule: has_firewall_rule failed (error 5)",
        ),
        (
            "add_firewall_rule",
            "adding the firewall rule: add_firewall_rule failed (error 5)",
        ),
        (
            "start_service",
            "starting MouserService: start_service failed (error 5)",
        ),
    ] {
        let fake = FakeWin::new();
        fake.fail(method, 5);
        assert_eq!(
            run(install_on(&fake)),
            (1, format!("{reason}\n")),
            "{method}"
        );
    }
}

#[test]
fn uninstall_stops_the_service_waits_for_it_and_deletes_it() {
    let fake = installed();
    assert_eq!(run(uninstall(&fake, ME)), (0, String::new()));
    let calls = fake.calls();
    let stop = calls
        .iter()
        .position(|c| *c == Call::StopService("MouserService".into()))
        .unwrap();
    assert_eq!(calls[stop + 1], Call::DeleteService("MouserService".into()));
    assert_eq!(fake.service("MouserService"), None);
    assert!(!fake.running().contains(&("mouser-service.exe".into(), 0)));
}

#[test]
fn uninstall_waits_for_a_service_that_takes_its_time_to_stop() {
    let fake = FakeWin::new();
    install_on(&fake).unwrap();
    // The service process is still ending its engine when the SCM returns.
    let service = fake.start_process("mouser-service.exe", 0);
    fake.slow_to_stop();
    let ending = std::thread::scope(|scope| {
        let uninstalling = scope.spawn(|| uninstall(&fake, ME));
        fake.await_calls(|calls| calls.contains(&Call::StopService("MouserService".into())));
        assert!(!uninstalling.is_finished());
        fake.exit(service, 0);
        uninstalling.join().unwrap()
    });
    assert_eq!(ending, Ok(()));
}

#[test]
fn uninstall_leaves_no_mouser_app_running_in_any_session_but_spares_itself() {
    let fake = installed();
    fake.start_process("mouser-app.exe", 1);
    fake.start_process("Mouser-App.exe", 2);
    fake.start_process("explorer.exe", 1);
    uninstall(&fake, ME).unwrap();
    let left: Vec<_> = fake.running().into_iter().map(|(name, _)| name).collect();
    assert_eq!(left, ["winlogon.exe", "explorer.exe"]);
}

#[test]
fn uninstall_removes_the_rule_and_the_policy_value_install_added() {
    let fake = installed();
    uninstall(&fake, ME).unwrap();
    assert_eq!(fake.rules(), []);
    assert_eq!(fake.dword(POLICY, "SoftwareSASGeneration"), None);
}

#[test]
fn uninstall_exits_0_when_the_service_rule_and_value_are_gone_already() {
    let fake = FakeWin::new();
    assert_eq!(run(uninstall(&fake, ME)), (0, String::new()));
    assert!(
        !fake
            .calls()
            .contains(&Call::DeleteFirewallRule("Mouser".into()))
    );
    // A stopped service, deleted before but not yet gone.
    let fake = FakeWin::default();
    install_on(&fake).unwrap();
    crate::winapi::WinApi::stop_service(&fake, "MouserService").unwrap();
    fake.fail("delete_service", 1072);
    assert_eq!(run(uninstall(&fake, ME)), (0, String::new()));
}

#[test]
fn a_mouser_process_that_ends_by_itself_meanwhile_is_no_failure() {
    let fake = installed();
    // The service's process ends as the uninstaller goes to wait for it.
    fake.slow_to_stop();
    fake.fail("open_process", 87);
    fake.start_process("mouser-app.exe", 1);
    fake.fail("terminate", 87);
    assert_eq!(run(uninstall(&fake, ME)), (0, String::new()));
}

#[test]
fn processes_that_cannot_be_listed_after_the_service_stopped_fail_the_uninstall() {
    let fake = installed();
    fake.pass("processes");
    fake.fail("processes", 5);
    assert_eq!(
        run(uninstall(&fake, ME)),
        (
            1,
            "ending mouser-app.exe: processes failed (error 5)\n".into()
        )
    );
}

#[test]
fn a_step_of_uninstall_that_fails_exits_1_with_its_reason() {
    for (method, reason) in [
        (
            "stop_service",
            "stopping MouserService: stop_service failed (error 5)",
        ),
        (
            "processes",
            "stopping MouserService: processes failed (error 5)",
        ),
        (
            "delete_service",
            "deleting MouserService: delete_service failed (error 5)",
        ),
        (
            "terminate",
            "ending mouser-app.exe: terminate failed (error 5)",
        ),
        (
            "has_firewall_rule",
            "removing the firewall rule: has_firewall_rule failed (error 5)",
        ),
        (
            "delete_firewall_rule",
            "removing the firewall rule: delete_firewall_rule failed (error 5)",
        ),
        (
            "delete_value",
            "removing SoftwareSASGeneration: delete_value failed (error 5)",
        ),
    ] {
        let fake = installed();
        fake.start_process("mouser-app.exe", 1);
        fake.fail(method, 5);
        assert_eq!(
            run(uninstall(&fake, ME)),
            (1, format!("{reason}\n")),
            "{method}"
        );
    }
}

const MACHINE: &str = r"C:\ProgramData\Mouser";

/// `--install` from `DIR`, its identity copied without a hitch.
fn install_on(fake: &FakeWin) -> Result<(), String> {
    install(fake, &dir(), Path::new(MACHINE), &|| Ok(()))
}

#[test]
fn install_makes_the_secrets_private_to_system_and_administrators_before_the_identity_arrives() {
    let fake = FakeWin::new();
    let copied = std::cell::Cell::new(false);
    let migrate = || {
        let calls = fake.calls();
        assert!(calls.contains(&Call::ProtectDir(
            Path::new(MACHINE).join("secrets"),
            "D:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)".into()
        )));
        assert!(!calls.contains(&Call::StartService("MouserService".into())));
        copied.set(true);
        Ok(())
    };
    install(&fake, &dir(), Path::new(MACHINE), &migrate).unwrap();
    assert!(copied.get());
    assert_eq!(
        fake.calls().last(),
        Some(&Call::StartService("MouserService".into()))
    );
}

#[test]
fn an_identity_that_cannot_be_copied_leaves_the_computer_to_rejoin_its_desk() {
    let fake = FakeWin::new();
    let unreadable = || Err("device_key is not in the user's store".to_string());
    assert_eq!(
        run(install(&fake, &dir(), Path::new(MACHINE), &unreadable)),
        (0, String::new())
    );
    assert_eq!(fake.service("MouserService"), Some((spec(), true)));
}

#[test]
fn a_secrets_folder_that_cannot_be_made_private_fails_the_install() {
    let fake = FakeWin::new();
    fake.fail("protect_dir", 5);
    assert_eq!(
        run(install_on(&fake)),
        (
            1,
            format!(
                r"protecting {MACHINE}\secrets: protect_dir failed (error 5)
"
            )
        )
    );
    assert!(
        !fake
            .calls()
            .contains(&Call::StartService("MouserService".into()))
    );
}

#[test]
fn uninstall_leaves_the_machine_folder_and_its_identity_alone() {
    let fake = installed();
    let before = fake.calls().len();
    uninstall(&fake, ME).unwrap();
    assert!(
        !fake.calls()[before..]
            .iter()
            .any(|c| matches!(c, Call::ProtectDir(..)))
    );
}
