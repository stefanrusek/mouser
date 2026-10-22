use mouser_platform::{Capability, Limitation, Os, PlatformInfo, SessionType};
use mouser_proto::control::ProblemKind;
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

#[tokio::test]
async fn every_limitation_is_a_problem_and_listed_on_the_computer() {
    let lab = Lab::new();
    let platform = VirtualPlatform::new();
    let all = [
        (Capability::Capture, "capture text"),
        (
            Capability::Inject,
            "Applications that ignore simulated input are not reached.",
        ),
        (Capability::Clipboard, "clipboard text"),
        (Capability::FileDrag, "drag text"),
        (Capability::SecureAttention, "cad text"),
    ];
    platform.set_info(info(&all));
    let node = lab.start_with("Alpha", platform.clone(), |_| {}).await;
    let s = status(&node).await;
    let mut texts: Vec<_> = s
        .problems
        .iter()
        .filter(|p| p.kind == ProblemKind::PlatformLimitation)
        .map(|p| {
            assert_eq!(p.fix, None);
            (p.message.clone(), p.computer)
        })
        .collect();
    texts.sort();
    let mut want: Vec<_> = all
        .iter()
        .map(|(_, t)| ((*t).to_string(), node.engine.computer_id()))
        .collect();
    want.sort();
    assert_eq!(texts, want);
    let mut listed = s.computers[0].limitations.clone();
    listed.sort();
    let mut texts: Vec<_> = all.iter().map(|(_, t)| (*t).to_string()).collect();
    texts.sort();
    assert_eq!(listed, texts);
}

#[tokio::test]
async fn a_problem_the_platform_notices_is_raised_once_with_its_text() {
    let lab = Lab::new();
    let platform = VirtualPlatform::new();
    let node = lab.start_with("Alpha", platform.clone(), |_| {}).await;
    let mut notes = node.engine.control().notifications();
    let text = "Mouser lost its hold on the keyboard and mouse and took it back.";
    platform.report_problem(text);
    platform.report_problem(text);
    let problem = recv_topic(&mut notes, mouser_proto::control::Topic::Problem).await;
    assert_eq!(problem["kind"], "platform_limitation");
    assert_eq!(
        problem["problem_id"],
        "platform_limitation:platform_problem"
    );
    assert_eq!(problem["message"], text);
    let s = status(&node).await;
    let raised: Vec<_> = s
        .problems
        .iter()
        .filter(|p| p.problem_id == "platform_limitation:platform_problem")
        .collect();
    assert_eq!(raised.len(), 1);
    assert_eq!(raised[0].computer, node.engine.computer_id());
}
