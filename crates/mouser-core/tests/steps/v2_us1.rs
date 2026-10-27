//! Step definitions for `features/v2_us1.feature`: the desk user's pointer and
//! keyboard keep working whatever goes wrong inside Mouser. Faults are applied
//! through the virtual computers: the operating system refusing input
//! (`refuse_input`), a panic inside an inject (`panic_on_next_inject`),
//! the backend losing capture (`lose_capture`) and a black-holed network.
//! "The pointer is visible" is `VirtualPlatform::pointer_hidden() == false`.

use std::collections::BTreeSet;
use std::fmt;
use std::time::Duration;

use cucumber::{World, given, then, when};
use mouser_core::EngineEvent;
use mouser_platform::{CaptureLossReason, LocalInput};
use mouser_proto::control::{ConnectionState, Empty, Method};
use mouser_testkit::{Injected, InputKind, VirtualPlatform};
use serde_json::json;

use crate::steps::us3::{form_desk, push_back, push_toward, until};
use crate::support::desk::Lab;

const A: u32 = 0x0007_0004;
const C: u32 = 0x0007_0006;
const ESC: u32 = 0x0007_0029;

const EVERY_KIND: [InputKind; 4] = [
    InputKind::Button,
    InputKind::Key,
    InputKind::Motion,
    InputKind::Scroll,
];

/// Time for what was sent to have been handled (and skipped).
const HANDLED: Duration = Duration::from_millis(300);

/// The state shared by the steps of one scenario.
#[derive(World)]
#[world(init = Self::new)]
pub struct PointerWorld {
    pub lab: Lab,
    /// What the computer that refused input injected while it refused.
    injected_while_refusing: Vec<Injected>,
    /// For each way control was taken back in the scenario: whether the
    /// pointer was visible and the mouse and keyboard acted on Alpha.
    returns: Vec<(&'static str, bool, bool)>,
}

impl fmt::Debug for PointerWorld {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "PointerWorld({:?})", self.lab.names())
    }
}

/// Keys and buttons still down after `injected`.
fn held(injected: &[Injected]) -> (BTreeSet<u32>, BTreeSet<u8>) {
    let (mut keys, mut buttons) = (BTreeSet::new(), BTreeSet::new());
    for event in injected {
        match event {
            Injected::Key { usage, down: true } => {
                keys.insert(*usage);
            }
            Injected::Key { usage, down: false } => {
                keys.remove(usage);
            }
            Injected::Button { button, down: true } => {
                buttons.insert(*button);
            }
            Injected::Button {
                button,
                down: false,
            } => {
                buttons.remove(button);
            }
            _ => {}
        }
    }
    (keys, buttons)
}

fn nothing_held() -> (BTreeSet<u32>, BTreeSet<u8>) {
    (BTreeSet::new(), BTreeSet::new())
}

fn tap(platform: &VirtualPlatform, usage: u32) {
    for down in [true, false] {
        platform.send_input(LocalInput::Key { usage, down });
    }
}

fn entered(platform: &VirtualPlatform) -> bool {
    platform
        .injected()
        .iter()
        .any(|i| matches!(i, Injected::MotionAbs { .. }))
}

impl PointerWorld {
    fn new() -> Self {
        Self {
            lab: Lab::new(),
            injected_while_refusing: Vec::new(),
            returns: Vec::new(),
        }
    }

    fn platform(&mut self, name: &str) -> VirtualPlatform {
        self.lab.node(name).platform.clone()
    }

    async fn desk(&mut self) {
        form_desk(&mut self.lab, &["Alpha", "Beta"]).await;
    }

    /// Wait until `name` sees the other computer as `connected` or not.
    async fn link(&mut self, name: &str, connected: bool) {
        self.lab
            .node(name)
            .wait_status(&format!("connected = {connected}"), |s| {
                s.computers
                    .iter()
                    .filter(|c| c.connection == ConnectionState::Connected && c.displays > 0)
                    .count()
                    == usize::from(connected)
            })
            .await;
    }

    /// The desk user pushes the shared mouse of Alpha onto Beta, and waits
    /// until Beta has the pointer.
    async fn cross_to_beta(&mut self) {
        let (alpha, beta) = (self.platform("Alpha"), self.platform("Beta"));
        beta.take_injected();
        push_toward("Alpha", &alpha);
        until("Beta to have the pointer", || {
            alpha.is_grabbed() && entered(&beta)
        })
        .await;
    }

    async fn alpha_drives_beta(&mut self) {
        self.desk().await;
        self.cross_to_beta().await;
    }

    /// Wait for both computers to be connected again, then check the desk
    /// user can cross to Beta and back without anything being restarted.
    async fn crossing_works(&mut self) {
        self.link("Alpha", true).await;
        self.link("Beta", true).await;
        self.cross_to_beta().await;
        let alpha = self.platform("Alpha");
        push_back("Alpha", &alpha);
        until("the pointer to come back to Alpha", || {
            !alpha.pointer_hidden()
        })
        .await;
    }

    /// Whether the pointer is visible on Alpha and Alpha's mouse and
    /// keyboard act on Alpha.
    fn acts_here(&mut self) -> (bool, bool) {
        let alpha = self.platform("Alpha");
        let visible = !alpha.pointer_hidden();
        let acts = alpha.send_input(LocalInput::Key {
            usage: C,
            down: true,
        }) && alpha.send_input(LocalInput::Key {
            usage: C,
            down: false,
        }) && alpha.send_input(LocalInput::Motion {
            dx: 1.0,
            dy: 0.0,
            abs: (960.0, 540.0),
        });
        (visible, acts)
    }

    async fn returned(&mut self, how: &'static str) {
        let alpha = self.platform("Alpha");
        until("the pointer to be visible on Alpha", || {
            !alpha.pointer_hidden()
        })
        .await;
        let (visible, acts) = self.acts_here();
        self.returns.push((how, visible, acts));
    }
}

// ---- Story 1 scenario 1 ------------------------------------------------------------------

#[given("this computer has the pointer from another desk computer")]
async fn has_the_pointer_from_another(w: &mut PointerWorld) {
    // "This computer" is Beta; Alpha is where the desk user is.
    w.alpha_drives_beta().await;
}

#[when("its operating system refuses a click, key, scroll or pointer move that Mouser passes on")]
async fn the_system_refuses(w: &mut PointerWorld) {
    let (alpha, beta) = (w.platform("Alpha"), w.platform("Beta"));
    beta.take_injected();
    beta.refuse_input(&EVERY_KIND);
    for down in [true, false] {
        alpha.send_input(LocalInput::Button { button: 1, down });
    }
    tap(&alpha, A);
    alpha.send_input(LocalInput::Scroll { dx: 0.0, dy: -3.0 });
    alpha.send_input(LocalInput::Motion {
        dx: 50.0,
        dy: 0.0,
        abs: (960.0, 540.0),
    });
    tokio::time::sleep(HANDLED).await;
    w.injected_while_refusing = beta.injected();
    beta.accept_input();
}

#[then(
    "that input is skipped, every later input the operating system accepts still arrives, and the desk stays connected"
)]
async fn skipped_and_later_input_arrives(w: &mut PointerWorld) {
    let (alpha, beta) = (w.platform("Alpha"), w.platform("Beta"));
    assert_eq!(
        w.injected_while_refusing,
        Vec::new(),
        "refused input was skipped"
    );
    tap(&alpha, C);
    until("the later key to arrive", || {
        beta.injected().contains(&Injected::Key {
            usage: C,
            down: false,
        })
    })
    .await;
    assert!(beta.injected().contains(&Injected::Key {
        usage: C,
        down: true
    }));
    w.link("Alpha", true).await;
    w.link("Beta", true).await;
}

// ---- Story 1 scenario 2 ------------------------------------------------------------------

#[given("a fault occurs inside Mouser on a desk computer")]
async fn a_fault_occurs(w: &mut PointerWorld) {
    w.alpha_drives_beta().await;
    let (alpha, beta) = (w.platform("Alpha"), w.platform("Beta"));
    beta.panic_on_next_inject();
    alpha.send_input(LocalInput::Key {
        usage: A,
        down: true,
    });
    w.lab
        .node("Beta")
        .engine_event("the fault", 0, |e| matches!(e, EngineEvent::Fault { .. }))
        .await;
}

#[when("the desk user keeps working")]
async fn the_desk_user_keeps_working(w: &mut PointerWorld) {
    let alpha = w.platform("Alpha");
    until("control to come back to Alpha", || !alpha.pointer_hidden()).await;
    alpha.send_input(LocalInput::Key {
        usage: A,
        down: false,
    });
    w.link("Alpha", true).await;
    w.link("Beta", true).await;
}

#[then(
    "crossing to and from that computer, typing on it, the panic key and pausing keep working without Mouser being restarted"
)]
async fn everything_keeps_working(w: &mut PointerWorld) {
    let (alpha, beta) = (w.platform("Alpha"), w.platform("Beta"));
    // Crossing to and from, and typing there.
    w.cross_to_beta().await;
    tap(&alpha, C);
    until("typing to arrive on Beta", || {
        beta.injected().contains(&Injected::Key {
            usage: C,
            down: false,
        })
    })
    .await;
    push_back("Alpha", &alpha);
    until("crossing back", || !alpha.pointer_hidden()).await;
    // The panic key.
    w.cross_to_beta().await;
    tap(&alpha, ESC);
    tap(&alpha, ESC);
    until("the panic key to bring the pointer back", || {
        !alpha.pointer_hidden()
    })
    .await;
    // Pausing.
    w.cross_to_beta().await;
    let _: Empty = w
        .lab
        .node("Alpha")
        .call(Method::ActivePause, json!({}))
        .await;
    until("pausing to bring the pointer back", || {
        !alpha.pointer_hidden()
    })
    .await;
    let _: Empty = w
        .lab
        .node("Alpha")
        .call(Method::ActiveResume, json!({}))
        .await;
}

// ---- Story 1 scenario 3 ------------------------------------------------------------------

#[given("the pointer has left this computer for another desk computer")]
async fn the_pointer_has_left(w: &mut PointerWorld) {
    w.alpha_drives_beta().await;
}

#[when("this computer's mouse and keyboard stop reaching Mouser")]
async fn mouse_and_keyboard_stop_reaching_mouser(w: &mut PointerWorld) {
    w.platform("Alpha")
        .lose_capture(CaptureLossReason::HooksSilent);
}

#[then(
    "the pointer is visible again on this computer's display and the mouse and keyboard act on this computer"
)]
async fn visible_again_and_acting_here(w: &mut PointerWorld) {
    let (alpha, beta) = (w.platform("Alpha"), w.platform("Beta"));
    until("the pointer to be visible", || !alpha.pointer_hidden()).await;
    let (visible, acts) = w.acts_here();
    assert!(visible && acts);
    tokio::time::sleep(HANDLED).await;
    assert!(
        !beta
            .injected()
            .iter()
            .any(|i| matches!(i, Injected::Key { usage: C, .. })),
        "nothing Alpha typed afterwards went to Beta"
    );
}

// ---- Story 1 scenarios 4, 5 and the quit edge case --------------------------------------

#[given("the pointer is on another desk computer")]
async fn the_pointer_is_on_another(w: &mut PointerWorld) {
    w.alpha_drives_beta().await;
}

#[when("the connection to it is lost for any reason, including a fault on either computer")]
async fn the_connection_is_lost(w: &mut PointerWorld) {
    // The network drops everything to and from it.
    w.lab.node("Beta").sim.set_blackhole(true);
    w.returned("unreachable").await;
    w.lab.node("Beta").sim.set_blackhole(false);
    // A fault on it.
    w.link("Alpha", true).await;
    w.link("Beta", true).await;
    w.cross_to_beta().await;
    let (alpha, beta) = (w.platform("Alpha"), w.platform("Beta"));
    beta.panic_on_next_inject();
    tap(&alpha, A);
    w.returned("fault").await;
}

#[then(
    "the pointer is visible on the computer the desk user is at and the mouse and keyboard act there"
)]
async fn visible_and_acting_there(w: &mut PointerWorld) {
    assert_eq!(w.returns.len(), 2, "{:?}", w.returns);
    for (how, visible, acts) in &w.returns {
        assert!(*visible && *acts, "after {how}");
    }
}

#[when("Mouser on the computer the desk user is at quits")]
async fn mouser_quits_where_the_user_is(w: &mut PointerWorld) {
    let (alpha, beta) = (w.platform("Alpha"), w.platform("Beta"));
    alpha.send_input(LocalInput::Key {
        usage: A,
        down: true,
    });
    alpha.send_input(LocalInput::Button {
        button: 1,
        down: true,
    });
    until("the key and button to be down on Beta", || {
        held(&beta.injected()) == (BTreeSet::from([A]), BTreeSet::from([1]))
    })
    .await;
    w.lab.node("Alpha").stop().await;
}

#[then(
    "the pointer is visible on that computer and no key or mouse button stays held down on any desk computer"
)]
async fn visible_and_nothing_held(w: &mut PointerWorld) {
    let (alpha, beta) = (w.platform("Alpha"), w.platform("Beta"));
    assert!(!alpha.pointer_hidden());
    until("Beta to let go of the key and button", || {
        held(&beta.injected()) == nothing_held()
    })
    .await;
    assert_eq!(held(&alpha.injected()), nothing_held());
}

#[given("Mouser quits on a computer that has the pointer from another computer")]
async fn mouser_quits_on_the_driven_computer(w: &mut PointerWorld) {
    w.alpha_drives_beta().await;
    w.lab.node("Beta").stop().await;
}

#[when("the other computer sees it leave")]
async fn the_other_computer_sees_it_leave(w: &mut PointerWorld) {
    w.link("Alpha", false).await;
}

#[then("the pointer and keyboard are back on the computer the desk user is at")]
async fn back_where_the_user_is(w: &mut PointerWorld) {
    let alpha = w.platform("Alpha");
    until("the pointer to be visible", || !alpha.pointer_hidden()).await;
    let (visible, acts) = w.acts_here();
    assert!(visible && acts);
}

// ---- Story 1 scenario 7 and the regression -----------------------------------------------

#[given("a desk link was lost for any reason, including a fault on either computer")]
async fn a_link_was_lost(w: &mut PointerWorld) {
    w.desk().await;
    w.lab.node("Beta").sim.set_blackhole(true);
    w.link("Alpha", false).await;
}

#[when("the two computers can reach each other again")]
async fn they_can_reach_each_other_again(w: &mut PointerWorld) {
    w.lab.node("Beta").sim.set_blackhole(false);
}

#[then("the desk user can cross between them without restarting Mouser on either")]
async fn can_cross_between_them(w: &mut PointerWorld) {
    w.crossing_works().await;
}

#[given("the desk user has crossed to another computer and the crossing is held")]
async fn crossed_and_held(w: &mut PointerWorld) {
    w.alpha_drives_beta().await;
    let alpha = w.platform("Alpha");
    alpha.send_input(LocalInput::Key {
        usage: A,
        down: true,
    });
}

#[given(
    "the operating system on that computer refuses an input Mouser passes on while the crossing is held"
)]
async fn the_system_refuses_while_held(w: &mut PointerWorld) {
    let (alpha, beta) = (w.platform("Alpha"), w.platform("Beta"));
    beta.refuse_input(&EVERY_KIND);
    tap(&alpha, C);
    tokio::time::sleep(HANDLED).await;
    beta.accept_input();
}

#[given("the link between the two computers then closes")]
async fn the_link_then_closes(w: &mut PointerWorld) {
    w.lab.node("Beta").sim.set_blackhole(true);
    w.link("Alpha", false).await;
    let alpha = w.platform("Alpha");
    until("the pointer to be visible", || !alpha.pointer_hidden()).await;
}

#[then("the desk user crosses between them without restarting Mouser on either")]
async fn crosses_between_them(w: &mut PointerWorld) {
    w.crossing_works().await;
}

// ---- Story 1 scenario 8 ------------------------------------------------------------------

#[given("the pointer is on another desk computer that has refused input or had a fault")]
async fn the_pointer_is_on_a_troubled_computer(w: &mut PointerWorld) {
    w.alpha_drives_beta().await;
    let (alpha, beta) = (w.platform("Alpha"), w.platform("Beta"));
    beta.refuse_input(&EVERY_KIND);
    tap(&alpha, C);
    tokio::time::sleep(HANDLED).await;
}

#[when(
    regex = r"^the desk user presses the panic key or the enable/disable hotkey, or chooses Pause from the menu-bar or tray menu, on the computer they are at$"
)]
async fn the_user_takes_control_back(w: &mut PointerWorld) {
    let alpha = w.platform("Alpha");
    // The panic key.
    tap(&alpha, ESC);
    tap(&alpha, ESC);
    w.returned("the panic key").await;
    // Pause (the menu-bar and tray item, and the enable/disable hotkey the
    // app registers, are all `active.pause`). Beta still refuses input, so
    // it records no entry: the crossing shows in Alpha's grab.
    push_toward("Alpha", &alpha);
    until("Alpha's input to be on Beta", || alpha.is_grabbed()).await;
    let _: Empty = w
        .lab
        .node("Alpha")
        .call(Method::ActivePause, json!({}))
        .await;
    w.returned("pause").await;
}

#[then("the pointer and keyboard are back on that computer")]
async fn back_on_that_computer(w: &mut PointerWorld) {
    assert_eq!(w.returns.len(), 2, "{:?}", w.returns);
    for (how, visible, acts) in &w.returns {
        assert!(*visible && *acts, "after {how}");
    }
}

// ---- Edge case: held when the operating system starts refusing ---------------------------

#[given(
    "a key or mouse button is held when the operating system starts refusing input on the computer with the pointer"
)]
async fn held_when_the_system_starts_refusing(w: &mut PointerWorld) {
    w.alpha_drives_beta().await;
    let (alpha, beta) = (w.platform("Alpha"), w.platform("Beta"));
    alpha.send_input(LocalInput::Key {
        usage: A,
        down: true,
    });
    alpha.send_input(LocalInput::Button {
        button: 1,
        down: true,
    });
    until("the key and button to be down on Beta", || {
        held(&beta.injected()) == (BTreeSet::from([A]), BTreeSet::from([1]))
    })
    .await;
    beta.refuse_input(&EVERY_KIND);
}

#[when("control leaves that computer")]
async fn control_leaves(w: &mut PointerWorld) {
    let alpha = w.platform("Alpha");
    push_back("Alpha", &alpha);
    until("control to leave Beta", || !alpha.pointer_hidden()).await;
    tokio::time::sleep(HANDLED).await;
}

#[then("no key or button stays held on any computer")]
async fn nothing_stays_held(w: &mut PointerWorld) {
    let (alpha, beta) = (w.platform("Alpha"), w.platform("Beta"));
    // The refused releases were dropped and Mouser's record of what Beta
    // holds is empty: once the system accepts input again, a second visit
    // that ends the same way has nothing left over to release.
    beta.accept_input();
    w.cross_to_beta().await;
    push_back("Alpha", &alpha);
    until("control to leave Beta again", || !alpha.pointer_hidden()).await;
    tokio::time::sleep(HANDLED).await;
    assert_eq!(held(&beta.injected()), nothing_held());
    assert!(
        !beta.injected().iter().any(|i| matches!(
            i,
            Injected::Key { down: false, .. } | Injected::Button { down: false, .. }
        )),
        "nothing recorded as held was left to release: {:?}",
        beta.injected()
    );
    assert_eq!(held(&alpha.injected()), nothing_held());
}
