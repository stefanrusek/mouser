//! Contract tests for `mouser send-cad` (Story 12 sc. 4): it calls `send_cad`
//! on the running Mouser; a computer that cannot be sent Ctrl-Alt-Del gets a
//! plain message and exit 1.

use crate::support::Lab;
use mouser_platform::{Os, PlatformInfo, SessionType};
use mouser_testkit::{Injected, VirtualPlatform};

fn computer(os: Os) -> VirtualPlatform {
    let platform = VirtualPlatform::new();
    platform.set_info(PlatformInfo {
        os,
        version: "virtual".into(),
        session: SessionType::Native,
        limitations: Vec::new(),
    });
    platform.set_secure_attention_supported(os == Os::Windows);
    platform
}

#[tokio::test(flavor = "multi_thread")]
async fn send_cad_signals_a_windows_computer() {
    let lab = Lab::new();
    let platform = computer(Os::Windows);
    let alpha = lab.add_with("Alpha", platform.clone()).await;

    let out = alpha.mouser(&["send-cad"]).await;
    assert_eq!((out.code, out.stdout.as_str()), (0, "Sent Ctrl-Alt-Del\n"));
    assert!(platform.injected().contains(&Injected::SecureAttention));

    let json = alpha.mouser(&["--json", "send-cad"]).await;
    assert_eq!(json.code, 0);
    assert_eq!(json.json(), serde_json::json!({}));
    lab.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn send_cad_to_a_computer_that_cannot_receive_it_prints_a_plain_message_and_exits_1() {
    let lab = Lab::new();
    let platform = computer(Os::MacOs);
    let alpha = lab.add_with("Alpha", platform.clone()).await;

    let out = alpha.mouser(&["send-cad"]).await;
    assert_eq!(out.code, 1);
    assert_eq!(out.stdout, "");
    assert_eq!(out.stderr.trim(), "Ctrl-Alt-Del cannot be sent to Alpha.");
    assert!(!platform.injected().contains(&Injected::SecureAttention));
    lab.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn send_cad_exits_2_when_mouser_is_not_running() {
    let lab = Lab::new();
    let nobody = lab.offline("Nobody");
    let out = nobody.mouser(&["send-cad"]).await;
    assert_eq!(out.code, 2);
    assert_eq!(out.stderr.trim(), "Mouser is not running");
    assert!(nobody.mouser(&["--help"]).await.stdout.contains("send-cad"));
    lab.shutdown().await;
}
