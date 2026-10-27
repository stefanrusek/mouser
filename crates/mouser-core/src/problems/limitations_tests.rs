use mouser_platform::{Capability, Limitation, Os, PlatformInfo, SessionType};
use mouser_testkit::VirtualPlatform;

use crate::testing::*;

fn info(limitations: &[(Capability, &str)]) -> PlatformInfo {
    PlatformInfo {
        os: Os::Windows,
        version: "11".into(),
        session: SessionType::Native,
        limitations: limitations
            .iter()
            .map(|(capability, text)| Limitation {
                capability: *capability,
                text: (*text).into(),
            })
            .collect(),
    }
}

fn all_limitations() -> Vec<(Capability, &'static str)> {
    vec![
        (Capability::Capture, "capture text"),
        (
            Capability::Inject,
            "Applications that ignore simulated input are not reached.",
        ),
        (Capability::Clipboard, "clipboard text"),
        (Capability::FileDrag, "drag text"),
        (Capability::SecureAttention, "cad text"),
        (Capability::Install, "install text"),
        (Capability::LoginScreen, "login screen text"),
        (Capability::Tray, "tray text"),
    ]
}

fn texts(all: &[(Capability, &str)]) -> Vec<String> {
    let mut texts: Vec<_> = all.iter().map(|(_, t)| (*t).to_string()).collect();
    texts.sort();
    texts
}

fn limitation_problems(s: &mouser_proto::control::Status) -> Vec<&mouser_proto::control::Problem> {
    s.problems
        .iter()
        .filter(|p| p.problem_id.starts_with("platform_limitation:"))
        .collect()
}

#[tokio::test]
async fn start_up_limitations_are_listed_on_the_computer_and_never_raised_as_problems() {
    let lab = Lab::new();
    let platform = VirtualPlatform::new();
    let all = all_limitations();
    platform.set_info(info(&all));
    let node = lab.start_with("Alpha", platform.clone(), |_| {}).await;
    let s = status(&node).await;
    assert_eq!(
        limitation_problems(&s),
        Vec::<&mouser_proto::control::Problem>::new()
    );
    assert_eq!(s.problems, []);
    let mut listed = s.computers[0].limitations.clone();
    listed.sort();
    assert_eq!(listed, texts(&all));
}

#[tokio::test]
async fn a_desk_mates_limitations_are_listed_under_it_alone_and_raise_nothing() {
    let lab = Lab::new();
    let a = lab.start("Alpha").await;
    let platform = VirtualPlatform::new();
    let all = all_limitations();
    platform.set_info(info(&all));
    let b = lab.start_with("Beta", platform, |_| {}).await;
    let _: mouser_proto::control::Desk = call(
        &a,
        mouser_proto::control::Method::DeskCreate,
        serde_json::json!({"name": "Home"}),
    )
    .await;
    admit(&a, &b).await;
    eventually("Beta's limitations to arrive", || async {
        status(&a)
            .await
            .computers
            .iter()
            .any(|c| c.name == "Beta" && !c.limitations.is_empty())
    })
    .await;
    settle().await;
    for node in [&a, &b] {
        let s = status(node).await;
        assert_eq!(s.problems, [], "{}", node.engine.computer_id());
        for computer in &s.computers {
            let mut listed = computer.limitations.clone();
            listed.sort();
            if computer.name == "Beta" {
                assert_eq!(listed, texts(&all));
            } else {
                assert!(
                    listed.iter().all(|l| !texts(&all).contains(l)),
                    "{} lists {listed:?}",
                    computer.name
                );
            }
        }
    }
}

#[tokio::test]
async fn a_problem_the_platform_notices_is_raised_once_with_its_text() {
    let lab = Lab::new();
    let platform = VirtualPlatform::new();
    let node = lab.start_with("Alpha", platform.clone(), |_| {}).await;
    let mut notes = node.engine.control().notifications();
    let text = "Mouser lost its hold on the keyboard and mouse and took it back.";
    let other = "Mouser could not change the pointer.";
    platform.report_problem(text);
    platform.report_problem(text);
    platform.report_problem(other);
    let first = recv_topic(&mut notes, mouser_proto::control::Topic::Problem).await;
    assert_eq!(first["kind"], "platform_limitation");
    assert_eq!(first["message"], text);
    assert_eq!(
        first["problem_id"],
        "platform_limitation:platform_problem:dea7eb0649abe104"
    );
    let second = recv_topic(&mut notes, mouser_proto::control::Topic::Problem).await;
    assert_eq!(second["message"], other);
    assert_eq!(
        second["problem_id"],
        "platform_limitation:platform_problem:018a5d9e2709bc76"
    );
    let s = status(&node).await;
    let raised: Vec<_> = s
        .problems
        .iter()
        .filter(|p| {
            p.problem_id
                .starts_with("platform_limitation:platform_problem:")
        })
        .collect();
    assert_eq!(raised.len(), 2);
    assert!(
        raised
            .iter()
            .all(|p| p.computer == node.engine.computer_id())
    );
}

#[tokio::test]
async fn a_platform_problem_is_withdrawn_by_its_text_and_may_come_back() {
    let lab = Lab::new();
    let platform = VirtualPlatform::new();
    let node = lab.start_with("Alpha", platform.clone(), |_| {}).await;
    let mut notes = node.engine.control().notifications();
    let text = "Mouser lost its hold on the keyboard and mouse and took it back.";
    let prefix = "platform_limitation:platform_problem:";
    let open = |s: &mouser_proto::control::Status| {
        s.problems
            .iter()
            .filter(|p| p.problem_id.starts_with(prefix))
            .count()
    };
    // Clearing text that was never raised does nothing.
    platform.clear_problem("never raised");
    platform.report_problem(text);
    let raised = recv_topic(&mut notes, mouser_proto::control::Topic::Problem).await;
    assert!(raised["cleared_at"].is_null());
    assert_eq!(open(&status(&node).await), 1);
    platform.clear_problem(text);
    let cleared = recv_topic(&mut notes, mouser_proto::control::Topic::Problem).await;
    assert_eq!(cleared["problem_id"], raised["problem_id"]);
    assert!(!cleared["cleared_at"].is_null());
    assert_eq!(open(&status(&node).await), 0);
    // Cleared twice: still nothing more to say; raised again: announced again.
    platform.clear_problem(text);
    platform.report_problem(text);
    let again = recv_topic(&mut notes, mouser_proto::control::Topic::Problem).await;
    assert!(again["cleared_at"].is_null());
    assert_eq!(open(&status(&node).await), 1);
}
