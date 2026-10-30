use mouser_proto::service_ipc::SasResult;

use crate::fake::{Call, FakeWin};
use crate::sas::{POLICY_KEY, POLICY_VALUE, answer};
use crate::test_log::TestLog;

const KEY: &str = r"SOFTWARE\Microsoft\Windows\CurrentVersion\Policies\System";

#[test]
fn the_policy_is_this_computers_software_sas_generation() {
    assert_eq!(POLICY_KEY, KEY);
    assert_eq!(POLICY_VALUE, "SoftwareSASGeneration");
}

#[test]
fn services_and_both_may_send_ctrl_alt_del() {
    for policy in [1, 3] {
        let fake = FakeWin::new();
        let log = TestLog::new();
        fake.set_dword(KEY, "SoftwareSASGeneration", policy);
        assert_eq!(answer(&fake, &log.log), SasResult::Sent);
        assert_eq!(
            fake.calls(),
            [
                Call::ReadDword(KEY.into(), "SoftwareSASGeneration".into()),
                Call::SendSas
            ]
        );
        assert_eq!(log.events(), [r#"service.sas {"result":"sent"}"#]);
    }
}

#[test]
fn any_other_policy_or_none_blocks_it_and_it_is_not_sent() {
    for policy in [Some(0), Some(2), Some(4), None] {
        let fake = FakeWin::new();
        let log = TestLog::new();
        if let Some(policy) = policy {
            fake.set_dword(KEY, "SoftwareSASGeneration", policy);
        }
        assert_eq!(answer(&fake, &log.log), SasResult::PolicyBlocked);
        assert!(!fake.calls().contains(&Call::SendSas), "{policy:?}");
        assert_eq!(log.events(), [r#"service.sas {"result":"policy_blocked"}"#]);
    }
}

#[test]
fn a_policy_that_cannot_be_read_or_a_send_that_fails_says_why() {
    let fake = FakeWin::new();
    let log = TestLog::new();
    fake.fail("read_dword", 5);
    assert_eq!(
        answer(&fake, &log.log),
        SasResult::Failed("read_dword failed (error 5)".into())
    );
    fake.set_dword(KEY, "SoftwareSASGeneration", 1);
    fake.fail("send_sas", 1);
    assert_eq!(
        answer(&fake, &log.log),
        SasResult::Failed("send_sas failed (error 1)".into())
    );
    assert_eq!(
        log.events(),
        [
            r#"service.sas {"result":"failed"}"#,
            r#"service.sas {"result":"failed"}"#
        ]
    );
}
