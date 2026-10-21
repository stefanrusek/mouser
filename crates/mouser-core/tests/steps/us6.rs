//! Step definitions for `features/us6.feature`: copy and paste work between
//! computers.

use std::fmt;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use cucumber::{World, given, then, when};
use mouser_platform::{ClipRep, LocalInput, Platform};
use mouser_proto::control::{ConnectionState, Empty, JoinPending, Method, PendingJoin, Topic};
use mouser_testkit::{Injected, VirtualPlatform};
use serde_json::json;

use crate::support::desk::{Lab, Setup};

const WAIT: Duration = Duration::from_secs(20);

const CTRL: u32 = 0x0007_00E0;
const ALT: u32 = 0x0007_00E2;
const SHIFT: u32 = 0x0007_00E1;
const V: u32 = 0x0007_0019;

/// The state shared by the steps of one scenario.
#[derive(World)]
#[world(init = Self::new)]
pub struct ClipboardWorld {
    pub lab: Lab,
    /// The copied files' home, kept alive for the scenario.
    source: Option<tempfile::TempDir>,
    /// What the other computers held before the targeted paste.
    before: Vec<(String, Vec<ClipRep>, u64)>,
}

impl fmt::Debug for ClipboardWorld {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ClipboardWorld({:?})", self.lab.names())
    }
}

async fn until(what: &str, mut done: impl FnMut() -> bool) {
    let end = Instant::now() + WAIT;
    while !done() {
        assert!(Instant::now() < end, "timed out waiting for {what}");
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

fn text(s: &str) -> Vec<ClipRep> {
    vec![ClipRep::Text(s.into())]
}

fn press(platform: &VirtualPlatform, keys: &[u32], then: u32) {
    for key in keys {
        platform.send_input(LocalInput::Key {
            usage: *key,
            down: true,
        });
    }
    platform.send_input(LocalInput::Key {
        usage: then,
        down: true,
    });
    platform.send_input(LocalInput::Key {
        usage: then,
        down: false,
    });
    for key in keys.iter().rev() {
        platform.send_input(LocalInput::Key {
            usage: *key,
            down: false,
        });
    }
}

impl ClipboardWorld {
    fn new() -> Self {
        Self {
            lab: Lab::new(),
            source: None,
            before: Vec::new(),
        }
    }

    fn platform(&mut self, name: &str) -> VirtualPlatform {
        self.lab.node(name).platform.clone()
    }

    async fn create_desk(&mut self, creator: &str, name: &str) {
        let _: mouser_proto::control::Desk = self
            .lab
            .node(creator)
            .call(Method::DeskCreate, json!({ "name": name }))
            .await;
    }

    async fn join(&mut self, member: &str, joiner: &str) {
        let target = self.lab.node(member).sim.clone();
        let address = self.lab.node(joiner).sim.route_to(&target).to_string();
        let _: JoinPending = self
            .lab
            .node(joiner)
            .call(Method::DeskJoin, json!({ "address": address }))
            .await;
        let value = self
            .lab
            .node(member)
            .notification("a join request", Topic::JoinRequest, |p| {
                p["computer"]["name"] == joiner
            })
            .await;
        let request: PendingJoin = serde_json::from_value(value).unwrap();
        let _: Empty = self
            .lab
            .node(member)
            .call(
                Method::JoinDecide,
                json!({"request_id": request.request_id, "approve": true}),
            )
            .await;
    }

    /// The named computers on one desk, fully connected and laid out.
    async fn desk(
        &mut self,
        name: &str,
        creator: &str,
        joiners: &[&str],
        setups: Vec<(&str, Setup)>,
    ) {
        for (name, setup) in setups {
            self.lab.add_with(name, setup).await;
        }
        self.create_desk(creator, name).await;
        for joiner in joiners {
            self.join(creator, joiner).await;
        }
        let all = joiners.len() + 1;
        for name in std::iter::once(&creator).chain(joiners) {
            let node = self.lab.node(name);
            node.wait_status("everyone connected with their displays", |s| {
                s.computers.len() == all
                    && s.computers.iter().all(|c| {
                        c.connection == ConnectionState::SelfComputer
                            || (c.connection == ConnectionState::Connected && c.displays > 0)
                    })
            })
            .await;
            until("the whole layout", || node.layout().placements.len() == all).await;
        }
    }

    async fn two(&mut self) {
        self.desk(
            "Home office",
            "Alpha",
            &["Beta"],
            vec![("Alpha", Setup::default()), ("Beta", Setup::default())],
        )
        .await;
    }

    /// Alpha's mouse crosses to Beta, which is now steered from Alpha's keyboard.
    async fn cross(&mut self) {
        let (alpha, beta) = (self.platform("Alpha"), self.platform("Beta"));
        alpha.send_input(LocalInput::Motion {
            dx: 1.0,
            dy: 0.0,
            abs: (1919.0, 540.0),
        });
        until("the pointer on Beta", || {
            alpha.is_grabbed()
                && beta
                    .injected()
                    .iter()
                    .any(|i| matches!(i, Injected::MotionAbs { .. }))
        })
        .await;
    }

    /// Ctrl+V on Alpha's keyboard, which now types on Beta.
    fn paste(&mut self) {
        press(&self.platform("Alpha"), &[CTRL], V);
    }

    async fn clipboard_is(&mut self, name: &str, expected: Vec<ClipRep>) {
        let platform = self.platform(name);
        until(&format!("{name}'s clipboard to hold the content"), || {
            platform.clipboard_contents() == expected
        })
        .await;
    }
}

fn change_count(platform: &VirtualPlatform) -> u64 {
    platform.clipboard().unwrap().change_count()
}

// ---- scenario 1: text and images --------------------------------------------------------

fn formatted() -> Vec<ClipRep> {
    vec![
        ClipRep::Text("hello".into()),
        ClipRep::Html("<b>hello</b>".into()),
        ClipRep::Rtf(b"{\\rtf1 {\\b hello}}".to_vec()),
        ClipRep::Png(vec![0x89, b'P', b'N', b'G', 1, 2, 3]),
    ]
}

#[given("the desk user copied plain text, formatted text, or an image on one computer")]
async fn copied_text_or_image(w: &mut ClipboardWorld) {
    w.two().await;
    w.platform("Alpha").set_local_clipboard(formatted());
}

#[when("they cross to another computer and paste")]
async fn cross_and_paste(w: &mut ClipboardWorld) {
    w.cross().await;
    w.paste();
}

#[then(
    "the same content is pasted, with formatting kept wherever the receiving application supports it"
)]
async fn same_content(w: &mut ClipboardWorld) {
    // Every representation is on Beta: the application picks the richest it supports.
    w.clipboard_is("Beta", formatted()).await;
}

// ---- scenario 2: files and folders ------------------------------------------------------

#[given("the desk user copied one or more files or folders")]
async fn copied_files(w: &mut ClipboardWorld) {
    w.two().await;
    let source = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(source.path().join("project/src")).unwrap();
    std::fs::write(source.path().join("project/readme.txt"), b"read me").unwrap();
    std::fs::write(source.path().join("project/src/main.rs"), b"fn main() {}").unwrap();
    std::fs::write(source.path().join("photo.png"), [1u8, 2, 3, 4]).unwrap();
    let paths: Vec<PathBuf> = vec![
        source.path().join("project"),
        source.path().join("photo.png"),
    ];
    w.source = Some(source);
    w.platform("Alpha")
        .set_local_clipboard(vec![ClipRep::Files(paths)]);
}

#[when("they paste them on another computer")]
async fn paste_files(w: &mut ClipboardWorld) {
    w.cross().await;
    w.paste();
}

#[then("copies of those files and folders appear there")]
async fn copies_appear(w: &mut ClipboardWorld) {
    let beta = w.platform("Beta");
    until("the files on Beta", || {
        matches!(&beta.clipboard_contents()[..], [ClipRep::Files(_)])
    })
    .await;
    let [ClipRep::Files(copies)] = &beta.clipboard_contents()[..] else {
        panic!("files");
    };
    let source = w.source.as_ref().unwrap().path();
    assert_eq!(copies.len(), 2);
    assert!(
        copies.iter().all(|c| !c.starts_with(source)),
        "copies, not the originals"
    );
    assert_eq!(copies[0].file_name().unwrap(), "project");
    assert_eq!(
        std::fs::read(copies[0].join("readme.txt")).unwrap(),
        b"read me"
    );
    assert_eq!(
        std::fs::read(copies[0].join("src/main.rs")).unwrap(),
        b"fn main() {}"
    );
    assert_eq!(std::fs::read(&copies[1]).unwrap(), [1u8, 2, 3, 4]);
}

// ---- scenario 3: targeted paste ---------------------------------------------------------

#[given("the desk user wants clipboard content on just one computer")]
async fn wants_content_on_one(w: &mut ClipboardWorld) {
    w.desk(
        "Home office",
        "Alpha",
        &["Beta", "Gamma"],
        vec![
            ("Alpha", Setup::default()),
            ("Beta", Setup::default()),
            ("Gamma", Setup::default()),
        ],
    )
    .await;
    // Alpha copies; then Beta's user copies something else, so Alpha and Gamma hold that.
    w.platform("Alpha")
        .set_local_clipboard(text("copied on Alpha"));
    w.clipboard_is("Beta", text("copied on Alpha")).await;
    w.clipboard_is("Gamma", text("copied on Alpha")).await;
    w.platform("Beta")
        .set_local_clipboard(text("copied on Beta"));
    w.clipboard_is("Alpha", text("copied on Beta")).await;
    w.clipboard_is("Gamma", text("copied on Beta")).await;
    for name in ["Alpha", "Gamma"] {
        let platform = w.platform(name);
        w.before.push((
            name.into(),
            platform.clipboard_contents(),
            change_count(&platform),
        ));
    }
}

#[when("they use the targeted paste hotkey on that computer")]
async fn targeted_hotkey(w: &mut ClipboardWorld) {
    press(&w.platform("Beta"), &[CTRL, ALT, SHIFT], V);
}

#[then(
    "it pastes the content copied on the other computer and no other computer's clipboard changes"
)]
async fn pastes_other_content(w: &mut ClipboardWorld) {
    w.clipboard_is("Beta", text("copied on Alpha")).await;
    // Long enough for a stray offer to have arrived.
    tokio::time::sleep(Duration::from_millis(500)).await;
    let before = std::mem::take(&mut w.before);
    for (name, contents, count) in before {
        let platform = w.platform(&name);
        assert_eq!(platform.clipboard_contents(), contents, "{name}");
        assert_eq!(change_count(&platform), count, "{name} was not written");
    }
}

// ---- edge case: unrepresentable format --------------------------------------------------

#[given("the clipboard contains a format the receiving computer cannot represent")]
async fn unrepresentable(w: &mut ClipboardWorld) {
    let plain_text_only = Setup {
        clipboard_holds: Some(|rep| matches!(rep, ClipRep::Text(_))),
        ..Setup::default()
    };
    w.desk(
        "Home office",
        "Alpha",
        &["Beta"],
        vec![("Alpha", Setup::default()), ("Beta", plain_text_only)],
    )
    .await;
    w.platform("Alpha").set_local_clipboard(vec![
        ClipRep::Text("words".into()),
        ClipRep::Html("<i>words</i>".into()),
        ClipRep::Rtf(b"{\\rtf1 {\\i words}}".to_vec()),
    ]);
}

#[when("the desk user pastes on the receiving computer")]
async fn pastes_on_receiver(w: &mut ClipboardWorld) {
    w.cross().await;
    w.paste();
}

#[then("the best available representation is pasted (for example plain text for formatted text)")]
async fn best_available(w: &mut ClipboardWorld) {
    w.clipboard_is("Beta", text("words")).await;
}

// ---- Story 1 scenario 4: clipboard isolation between desks ------------------------------

#[given(regex = r#"^two desks, "([^"]*)" and "([^"]*)", on one network$"#)]
async fn two_desks(w: &mut ClipboardWorld, first: String, second: String) {
    w.desk(
        &first,
        "Home1",
        &["Home2"],
        vec![("Home1", Setup::default()), ("Home2", Setup::default())],
    )
    .await;
    w.lab.add("Lab1").await;
    w.lab.add("Lab2").await;
    w.create_desk("Lab1", &second).await;
    w.join("Lab1", "Lab2").await;
    for name in ["Lab1", "Lab2"] {
        w.lab
            .node(name)
            .wait_status("the Lab desk connected", |s| {
                s.computers.len() == 2
                    && s.computers
                        .iter()
                        .all(|c| c.connection != ConnectionState::Offline)
            })
            .await;
    }
}

#[when(regex = r#"^a computer on "([^"]*)" is in use$"#)]
async fn lab_computer_in_use(w: &mut ClipboardWorld, _desk: String) {
    let lab1 = w.platform("Lab1");
    lab1.send_input(LocalInput::Motion {
        dx: 5.0,
        dy: 0.0,
        abs: (100.0, 100.0),
    });
    lab1.send_input(LocalInput::Key {
        usage: 0x0007_0004,
        down: true,
    });
    lab1.set_local_clipboard(text("secret"));
    // The desk-mate gets it, which shows clipboards do travel; the other desk,
    // which shares the network for long enough to have found it, gets nothing.
    w.clipboard_is("Lab2", text("secret")).await;
    tokio::time::sleep(Duration::from_millis(1500)).await;
}

#[then(
    regex = r#"^computers on "([^"]*)" receive none of its pointer movement, keystrokes, clipboard contents, or files, and neither desk lists the other's computers as members$"#
)]
async fn nothing_crosses(w: &mut ClipboardWorld, _desk: String) {
    for home in ["Home1", "Home2"] {
        let platform = w.platform(home);
        assert!(platform.injected().is_empty(), "{home} received input");
        assert!(
            platform.clipboard_contents().is_empty(),
            "{home} received clipboard contents"
        );
        assert!(
            platform.incoming_drags().is_empty(),
            "{home} received files"
        );
    }
    for (name, members) in [
        ("Home1", ["Home1", "Home2"]),
        ("Home2", ["Home1", "Home2"]),
        ("Lab1", ["Lab1", "Lab2"]),
        ("Lab2", ["Lab1", "Lab2"]),
    ] {
        let status = w.lab.node(name).status().await;
        let mut names: Vec<_> = status.computers.iter().map(|c| c.name.clone()).collect();
        names.sort();
        assert_eq!(names, members, "members listed on {name}");
    }
}
