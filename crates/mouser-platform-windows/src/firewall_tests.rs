use crate::firewall::*;
use mouser_platform::PlatformProblem;
use std::path::Path;
use std::sync::mpsc::channel;
use std::time::Duration;

const DOMAIN: i32 = 1;
const PRIVATE: i32 = 2;
const PUBLIC: i32 = 4;
const ALL: i32 = 0x7FFF_FFFF;

const EXE: &str = r"C:\Users\me\AppData\Local\Mouser\mouser-app.exe";

fn rule(program: &str, allow: bool, profiles: i32) -> Rule {
    Rule {
        program: program.into(),
        inbound: true,
        allow,
        enabled: true,
        profiles,
    }
}

fn policy(enabled: i32, current: i32, rules: Vec<Rule>) -> Policy {
    Policy {
        enabled,
        current,
        rules,
    }
}

fn blocked(policy: &Policy) -> bool {
    is_blocked(policy, Path::new(EXE), |s| s.to_string())
}

#[test]
fn the_problem_text_is_exact() {
    assert_eq!(
        BLOCKED,
        "Other computers can't reach this PC because Windows Firewall blocks Mouser. Allow Mouser in Windows Security > Firewall & network protection > Allow an app through firewall."
    );
}

#[test]
fn a_firewall_that_is_off_for_the_current_profile_blocks_nothing() {
    // The VM's state: Private off, Public on, on a Private network.
    assert!(!blocked(&policy(PUBLIC, PRIVATE, vec![])));
    assert!(!blocked(&policy(0, PUBLIC | PRIVATE | DOMAIN, vec![])));
    // No current profile at all (no network).
    assert!(!blocked(&policy(ALL, 0, vec![])));
}

#[test]
fn no_rule_for_mouser_on_a_firewalled_profile_is_blocked() {
    // The VM after a resume: Public network, Public profile on, no rule yet.
    assert!(blocked(&policy(PUBLIC, PUBLIC, vec![])));
    assert!(blocked(&policy(ALL, DOMAIN, vec![])));
}

#[test]
fn an_enabled_inbound_allow_rule_lets_mouser_in() {
    let allowed = |profiles| policy(PUBLIC, PUBLIC, vec![rule(EXE, true, profiles)]);
    assert!(!blocked(&allowed(PUBLIC)));
    assert!(!blocked(&allowed(ALL)));
    assert!(!blocked(&allowed(PUBLIC | PRIVATE)));
    // An allow rule for another profile does not help on this one.
    assert!(blocked(&allowed(PRIVATE)));
    assert!(blocked(&allowed(DOMAIN | PRIVATE)));
}

#[test]
fn every_current_firewalled_profile_needs_its_own_allow() {
    let both = |rules| policy(PUBLIC | PRIVATE, PUBLIC | PRIVATE, rules);
    assert!(blocked(&both(vec![rule(EXE, true, PUBLIC)])));
    assert!(blocked(&both(vec![rule(EXE, true, PRIVATE)])));
    assert!(!blocked(&both(vec![
        rule(EXE, true, PUBLIC),
        rule(EXE, true, PRIVATE)
    ])));
    assert!(!blocked(&both(vec![rule(EXE, true, ALL)])));
    // A profile that is current but whose firewall is off needs nothing.
    let one = policy(PUBLIC, PUBLIC | PRIVATE, vec![rule(EXE, true, PUBLIC)]);
    assert!(!blocked(&one));
    // A profile that is firewalled but not current needs nothing.
    let other = policy(PUBLIC | PRIVATE, PUBLIC, vec![rule(EXE, true, PUBLIC)]);
    assert!(!blocked(&other));
}

#[test]
fn an_inbound_block_rule_blocks_even_with_an_allow_rule() {
    let p = |rules| policy(PUBLIC, PUBLIC, rules);
    // What Windows leaves when the user cancels its prompt.
    assert!(blocked(&p(vec![rule(EXE, false, ALL)])));
    assert!(blocked(&p(vec![rule(EXE, false, PUBLIC)])));
    assert!(blocked(&p(vec![
        rule(EXE, true, ALL),
        rule(EXE, false, PUBLIC)
    ])));
    assert!(blocked(&p(vec![
        rule(EXE, false, PUBLIC),
        rule(EXE, true, ALL)
    ])));
    // A block rule for another profile leaves this one alone.
    assert!(!blocked(&p(vec![
        rule(EXE, true, PUBLIC),
        rule(EXE, false, PRIVATE)
    ])));
}

#[test]
fn disabled_outbound_and_other_programs_rules_do_not_count() {
    let p = |rules| policy(PUBLIC, PUBLIC, rules);
    let mut off = rule(EXE, true, ALL);
    off.enabled = false;
    assert!(blocked(&p(vec![off])));
    let mut out = rule(EXE, true, ALL);
    out.inbound = false;
    assert!(blocked(&p(vec![out])));
    assert!(blocked(&p(vec![rule(r"C:\other\app.exe", true, ALL)])));
    // A disabled or outbound Block rule does not block.
    let mut off_block = rule(EXE, false, ALL);
    off_block.enabled = false;
    let mut out_block = rule(EXE, false, ALL);
    out_block.inbound = false;
    assert!(!blocked(&p(vec![
        rule(EXE, true, ALL),
        off_block,
        out_block,
        rule(r"C:\other\app.exe", false, ALL)
    ])));
    // A rule about no program (a port, a service) says nothing about Mouser.
    assert!(blocked(&p(vec![rule("", true, ALL)])));
    assert!(!blocked(&p(vec![
        rule(EXE, true, ALL),
        rule("", false, ALL)
    ])));
}

#[test]
fn rule_paths_match_without_regard_to_case_slashes_quotes_or_variables() {
    let p = |program: &str| policy(PUBLIC, PUBLIC, vec![rule(program, true, ALL)]);
    assert!(!blocked(&p(&EXE.to_uppercase())));
    assert!(!blocked(&p(&EXE.to_lowercase())));
    assert!(!blocked(&p(&EXE.replace('\\', "/"))));
    assert!(!blocked(&p(&format!("\"{EXE}\""))));
    assert!(!blocked(&p(&format!(" {EXE} "))));
    assert!(blocked(&p(
        r"C:\Users\me\AppData\Local\Mouser\mouser-app.exe.old"
    )));
    let with_variable = p(r"%LocalAppData%\Mouser\MOUSER-APP.exe");
    let expand = |s: &str| s.replace("%LocalAppData%", r"C:\Users\me\AppData\Local");
    assert!(!is_blocked(&with_variable, Path::new(EXE), expand));
    // Unexpanded, it is some other path.
    assert!(blocked(&with_variable));
}

#[test]
fn variables_in_paths_are_expanded() {
    let root = std::env::var("SystemRoot").unwrap();
    assert_eq!(
        expand_variables(r"%SystemRoot%\x.exe"),
        format!(r"{root}\x.exe")
    );
    assert_eq!(expand_variables(r"C:\plain.exe"), r"C:\plain.exe");
    assert_eq!(expand_variables(""), "");
    assert_eq!(
        expand_variables(r"%MouserSurelyUnsetVariable%\a"),
        r"%MouserSurelyUnsetVariable%\a"
    );
}

#[test]
fn the_real_policy_reads_and_is_consistent() {
    let policy = read_policy().expect("the firewall policy is readable without elevation");
    assert_eq!(policy.enabled & !7, 0, "{:?}", policy.enabled);
    assert_eq!(policy.current & !7, 0, "{:?}", policy.current);
    assert!(
        policy.rules.iter().all(|r| r.profiles != 0),
        "a rule applies to some profile"
    );
    // Windows ships rules, and every one has a name for its program or none.
    assert!(!policy.rules.is_empty());
    // The answer for this very test binary agrees with judging what was read.
    let exe = std::env::current_exe().unwrap();
    let judged = is_blocked(&policy, &exe, expand_variables);
    // The policy can change between the two reads; it does not on a quiet PC.
    assert_eq!(check_now(&exe), Some(judged));
}

#[test]
fn a_change_of_the_answer_is_said_once_each_way() {
    let (tx, rx) = channel();
    let (said_tx, said) = channel();
    let mut answers = [
        Some(false),
        Some(true),
        Some(true),
        None,
        Some(false),
        Some(false),
    ]
    .into_iter();
    let thread = std::thread::spawn(move || {
        run(
            &rx,
            Duration::from_millis(5),
            || answers.next().unwrap_or(None),
            |p| said_tx.send(p).unwrap(),
        );
    });
    // Events and the period each trigger a check; here the period does.
    assert_eq!(
        said.recv_timeout(Duration::from_secs(5)).unwrap(),
        PlatformProblem::Raised(BLOCKED.into())
    );
    assert_eq!(
        said.recv_timeout(Duration::from_secs(5)).unwrap(),
        PlatformProblem::Cleared(BLOCKED.into())
    );
    // Dropping the events ends the run.
    drop(tx);
    thread.join().unwrap();
    // Nothing else was said: not on the repeat, not on the failed check.
    assert!(said.try_recv().is_err());
}

#[test]
fn network_events_trigger_a_check_and_a_burst_is_one_check() {
    let (tx, rx) = channel();
    // A burst already waiting when the run starts.
    for _ in 0..5 {
        tx.send(()).unwrap();
    }
    let (checks_tx, checks) = channel();
    let (said_tx, said) = channel();
    let thread = std::thread::spawn(move || {
        run(
            &rx,
            Duration::from_secs(3600),
            || {
                checks_tx.send(()).unwrap();
                Some(true)
            },
            |p| said_tx.send(p).unwrap(),
        );
    });
    // The check at the start, then one for the whole burst.
    checks.recv_timeout(Duration::from_secs(5)).unwrap();
    checks.recv_timeout(Duration::from_secs(5)).unwrap();
    drop(tx);
    thread.join().unwrap();
    assert!(checks.try_recv().is_err());
    assert_eq!(
        said.try_iter().collect::<Vec<_>>(),
        [PlatformProblem::Raised(BLOCKED.into())]
    );
}
#[test]
fn a_watch_starts_and_stops() {
    let (tx, rx) = channel();
    let watch = watch(std::env::current_exe().unwrap(), move |p| {
        let _ = tx.send(p);
    })
    .unwrap();
    // Whatever the firewall says about this PC, it is only ever this problem.
    if let Ok(p) = rx.recv_timeout(Duration::from_secs(10)) {
        assert_eq!(p, PlatformProblem::Raised(BLOCKED.into()));
    }
    drop(watch);
}
