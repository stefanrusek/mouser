use mouser_proto::control::{FixAction, Problem, ProblemKind, Topic};
use mouser_proto::wire::ComputerMode;
use mouser_testkit::Injected;

use crate::lock::LockExt;
use crate::testing::*;

const PORTABLE: &str = "Portable Mouser on Beta can't control regedit.exe because it runs as administrator. Restart Mouser as administrator to control it, or install Mouser to also control the login screen, permission prompts and Ctrl-Alt-Del.";
const NOT_PORTABLE: &str = "Mouser on Beta can't control regedit.exe because it runs as administrator. Install Mouser to control it.";

/// Alpha (Linux, installed) and Beta (Windows, running as `mode`).
async fn desk(lab: &Lab, mode: ComputerMode) -> Vec<TestNode> {
    let alpha = lab.start_version("Alpha", "0.3.0", true).await;
    let beta = lab.start_windows("Beta", mode, "0.3.0").await;
    desk_from(vec![alpha, beta]).await
}

fn elevated(status: &mouser_proto::control::Status) -> Vec<Problem> {
    status
        .problems
        .iter()
        .filter(|p| p.kind == ProblemKind::ElevatedApp)
        .cloned()
        .collect()
}

/// Alpha crosses onto Beta; Beta has had the pointer once Alpha is remote
/// and Beta moved its pointer.
async fn cross(nodes: &[TestNode]) {
    let before = nodes[1]
        .platform
        .injected()
        .iter()
        .filter(|i| matches!(i, Injected::MotionAbs { .. }))
        .count();
    push_right(&nodes[0]);
    eventually("Beta to have the pointer", || async {
        nodes[1]
            .platform
            .injected()
            .iter()
            .filter(|i| matches!(i, Injected::MotionAbs { .. }))
            .count()
            > before
    })
    .await;
}

async fn alpha_local(nodes: &[TestNode]) {
    eventually("Alpha to be local", || async {
        nodes[0]
            .engine
            .ctx
            .crossing
            .guard()
            .session
            .remote()
            .is_none()
    })
    .await;
}

#[tokio::test]
async fn portable_mouser_explains_and_offers_the_restart_once() {
    let lab = Lab::new();
    let nodes = desk(&lab, ComputerMode::Portable).await;
    let beta = nodes[1].engine.computer_id();
    let mut notes = nodes[0].engine.control().notifications();
    cross(&nodes).await;
    nodes[1]
        .platform
        .set_foreground_app(Some(admin_app("regedit.exe", true)));
    alpha_local(&nodes).await;

    // On the Windows computer.
    eventually("the problem on Beta", || async {
        elevated(&status(&nodes[1]).await).len() == 1
    })
    .await;
    let problem = elevated(&status(&nodes[1]).await).remove(0);
    assert_eq!(problem.problem_id, format!("elevated_app:{beta}"));
    assert_eq!(problem.computer, beta);
    assert_eq!(problem.app.as_deref(), Some("regedit.exe"));
    assert_eq!(problem.message, PORTABLE);
    assert_eq!(problem.fix, Some(FixAction::RestartElevated(beta)));
    // On the controller, under that computer, announced once.
    eventually("the problem on Alpha", || async {
        elevated(&status(&nodes[0]).await).len() == 1
    })
    .await;
    let seen = elevated(&status(&nodes[0]).await).remove(0);
    assert_eq!(
        (seen.computer, seen.app, seen.fix),
        (beta, problem.app, problem.fix)
    );
    let note = recv_topic(&mut notes, Topic::Problem).await;
    assert_eq!(note["problem_id"], format!("elevated_app:{beta}"));
    assert_eq!(note["app"], "regedit.exe");

    // Crossing again with the application still in front raises nothing new.
    cross(&nodes).await;
    alpha_local(&nodes).await;
    settle().await;
    assert_eq!(elevated(&status(&nodes[1]).await).len(), 1);
    assert_eq!(elevated(&status(&nodes[0]).await).len(), 1);
}

#[tokio::test]
async fn any_other_mode_says_to_install_and_offers_no_fix() {
    let lab = Lab::new();
    let nodes = desk(&lab, ComputerMode::PortableElevated).await;
    cross(&nodes).await;
    nodes[1]
        .platform
        .set_foreground_app(Some(admin_app("regedit.exe", true)));
    alpha_local(&nodes).await;
    eventually("the problem on Beta", || async {
        elevated(&status(&nodes[1]).await).len() == 1
    })
    .await;
    let problem = elevated(&status(&nodes[1]).await).remove(0);
    assert_eq!(problem.message, NOT_PORTABLE);
    assert_eq!(problem.fix, None);
    assert_eq!(problem.app.as_deref(), Some("regedit.exe"));
}

#[tokio::test]
async fn the_problem_clears_when_control_next_enters_without_an_outranking_foreground() {
    let lab = Lab::new();
    let nodes = desk(&lab, ComputerMode::Portable).await;
    cross(&nodes).await;
    nodes[1]
        .platform
        .set_foreground_app(Some(admin_app("regedit.exe", true)));
    alpha_local(&nodes).await;
    eventually("the problem on Alpha", || async {
        elevated(&status(&nodes[0]).await).len() == 1
    })
    .await;

    // The application is no longer in front.
    nodes[1].platform.set_foreground_app(None);
    cross(&nodes).await;
    eventually("the problem to clear on Beta", || async {
        elevated(&status(&nodes[1]).await).is_empty()
    })
    .await;
    eventually("the problem to clear on Alpha", || async {
        elevated(&status(&nodes[0]).await).is_empty()
    })
    .await;
    // Alpha still has the pointer on Beta.
    assert!(
        nodes[0]
            .engine
            .ctx
            .crossing
            .guard()
            .session
            .remote()
            .is_some()
    );
}
