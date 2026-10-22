//! Contract tests for `mouser pause` / `mouser resume` (Story 2 sc. 12): they
//! call `active.pause` / `active.resume` on the running Mouser.

use crate::support::Lab;
use mouser_proto::control::EngineState;

#[tokio::test(flavor = "multi_thread")]
async fn pause_and_resume_toggle_the_running_mouser() {
    let lab = Lab::new();
    let alpha = lab.add("Alpha").await;
    assert_eq!(alpha.status().await.state, EngineState::Active);

    let paused = alpha.mouser(&["pause"]).await;
    assert_eq!((paused.code, paused.stdout.as_str()), (0, "Paused\n"));
    assert_eq!(alpha.status().await.state, EngineState::Paused);

    let resumed = alpha.mouser(&["resume"]).await;
    assert_eq!((resumed.code, resumed.stdout.as_str()), (0, "Resumed\n"));
    assert_eq!(alpha.status().await.state, EngineState::Active);

    assert_eq!(
        alpha.mouser(&["--json", "pause"]).await.json(),
        serde_json::json!({})
    );
    assert_eq!(alpha.status().await.state, EngineState::Paused);
    assert_eq!(
        alpha.mouser(&["--json", "resume"]).await.json(),
        serde_json::json!({})
    );
    assert_eq!(alpha.status().await.state, EngineState::Active);
    lab.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn pause_and_resume_exit_2_when_mouser_is_not_running() {
    let lab = Lab::new();
    let nobody = lab.offline("Nobody");
    for command in ["pause", "resume"] {
        let out = nobody.mouser(&[command]).await;
        assert_eq!(out.code, 2, "{command}");
        assert_eq!(out.stdout, "");
        assert_eq!(out.stderr.trim(), "Mouser is not running");
    }
    let help = nobody.mouser(&["--help"]).await;
    assert!(help.stdout.contains("pause") && help.stdout.contains("resume"));
    lab.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn help_says_pause_and_resume_act_on_this_computer_only() {
    let lab = Lab::new();
    let nobody = lab.offline("Nobody");
    for command in ["pause", "resume"] {
        let help = nobody.mouser(&[command, "--help"]).await;
        assert_eq!(help.code, 0);
        assert!(help.stdout.contains("this computer"), "{}", help.stdout);
        assert!(!help.stdout.contains("every computer"), "{}", help.stdout);
    }
    let overview = nobody.mouser(&["--help"]).await;
    assert!(
        overview.stdout.contains("this computer"),
        "{}",
        overview.stdout
    );
    assert!(
        !overview.stdout.contains("every computer"),
        "{}",
        overview.stdout
    );
    lab.shutdown().await;
}
