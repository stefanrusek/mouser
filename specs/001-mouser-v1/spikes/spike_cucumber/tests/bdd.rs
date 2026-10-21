use cucumber::{given, then, when, World};
use spike_cucumber::Desk;

#[derive(Debug, Default, World)]
struct DeskWorld { desk: Desk }

#[given(expr = "a desk {string} created on {string}")]
fn create(w: &mut DeskWorld, name: String, host: String) { w.desk = Desk::create(&name, &host); }

#[when(expr = "{string} asks to join and {string} approves")]
fn join(w: &mut DeskWorld, who: String, _approver: String) { w.desk.approve_join(&who); }

#[then(expr = "the desk {string} has {int} members")]
fn count(w: &mut DeskWorld, name: String, n: usize) {
    assert_eq!(w.desk.name, name);
    assert_eq!(w.desk.members.len(), n);
}

#[tokio::main]
async fn main() {
    DeskWorld::cucumber().fail_on_skipped().run_and_exit("tests/features").await;
}
