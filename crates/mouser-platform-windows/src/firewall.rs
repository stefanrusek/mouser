//! Windows Firewall detection (read-only, no elevation).
//!
//! The first time Mouser listens, Windows asks whether to allow it through the
//! firewall; cancelling that prompt leaves a Block rule behind, and a profile
//! that was never allowed leaves the app unreachable. Either way other
//! computers cannot connect to this PC, and nothing in Mouser says why. This
//! reads the firewall policy (`INetFwPolicy2`) and raises a problem while the
//! firewall is on for a current network profile and Mouser is not let in on it.
//!
//! The decision is [`is_blocked`], a pure function over a [`Policy`]; reading
//! the real policy is [`read_policy`]. [`watch`] re-checks at the start, on
//! every network change (the crate's `netwatch`) and periodically, because
//! the network profile of an interface (Private to Public) can change
//! without an interface or address change, and Windows names a new network a
//! few seconds after the interface came up.

use crate::dropdir::Com;
use crate::inject::os;
use crate::netwatch;
use mouser_platform::{Error, PlatformProblem, Result, WatchHandle};
use std::path::Path;
use std::sync::mpsc::{Receiver, RecvTimeoutError, channel};
use std::time::Duration;
use windows::Win32::NetworkManagement::WindowsFirewall::{
    INetFwPolicy2, INetFwRule, NET_FW_ACTION_ALLOW, NET_FW_PROFILE2_DOMAIN,
    NET_FW_PROFILE2_PRIVATE, NET_FW_PROFILE2_PUBLIC, NET_FW_RULE_DIR_IN, NetFwPolicy2,
};
use windows::Win32::System::Com::{CLSCTX_ALL, CoCreateInstance, IDispatch};
use windows::Win32::System::Environment::ExpandEnvironmentStringsW;
use windows::Win32::System::Ole::IEnumVARIANT;
use windows::Win32::System::Variant::{VARIANT, VT_DISPATCH, VariantClear};
use windows::core::{Interface, PCWSTR};

/// The problem while the firewall keeps other computers out.
pub(crate) const BLOCKED: &str = "Other computers can't reach this PC because Windows Firewall blocks Mouser. Allow Mouser in Windows Security > Firewall & network protection > Allow an app through firewall.";

/// How often the policy is read when nothing else prompts it.
pub(crate) const RECHECK: Duration = Duration::from_secs(60);

/// A firewall rule, reduced to what decides whether Mouser is let in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Rule {
    /// The program the rule is about, as the rule has it (may hold
    /// `%VARIABLES%`); empty for a rule about no particular program.
    pub(crate) program: String,
    pub(crate) inbound: bool,
    pub(crate) allow: bool,
    pub(crate) enabled: bool,
    /// `NET_FW_PROFILE2_*` bits the rule applies to (`ALL` has every bit).
    pub(crate) profiles: i32,
}

/// The firewall as it stands now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Policy {
    /// Profiles the firewall is switched on for.
    pub(crate) enabled: i32,
    /// Profiles of the networks this PC is connected to now.
    pub(crate) current: i32,
    pub(crate) rules: Vec<Rule>,
}

/// Paths compare without regard to case or the kind of slash.
fn normal(path: &str) -> String {
    path.trim()
        .trim_matches('"')
        .replace('/', "\\")
        .to_lowercase()
}

/// Whether Mouser (`exe`) is kept out: for some network profile that is both
/// current and has the firewall on, Mouser has no enabled inbound Allow rule
/// or there is an enabled inbound Block rule (Block wins in Windows).
/// `expand` turns `%VARIABLES%` in a rule's program path into their values.
/// Only rules that name Mouser's program count; a rule about no program (a
/// port, a service) is not read as letting Mouser in or keeping it out.
pub(crate) fn is_blocked(policy: &Policy, exe: &Path, expand: impl Fn(&str) -> String) -> bool {
    let exe = normal(&exe.to_string_lossy());
    let mine: Vec<&Rule> = policy
        .rules
        .iter()
        .filter(|r| r.enabled && r.inbound && !r.program.is_empty())
        .filter(|r| normal(&expand(&r.program)) == exe)
        .collect();
    [
        NET_FW_PROFILE2_DOMAIN,
        NET_FW_PROFILE2_PRIVATE,
        NET_FW_PROFILE2_PUBLIC,
    ]
    .iter()
    .map(|p| p.0)
    .filter(|p| policy.current & policy.enabled & p != 0)
    .any(|p| {
        let covering = || mine.iter().filter(move |r| r.profiles & p != 0);
        covering().any(|r| !r.allow) || !covering().any(|r| r.allow)
    })
}

/// Expands `%VARIABLES%` in `text`; a variable that is not set stays as written.
pub(crate) fn expand_variables(text: &str) -> String {
    let wide: Vec<u16> = text.encode_utf16().chain(Some(0)).collect();
    let src = PCWSTR(wide.as_ptr());
    let need = unsafe { ExpandEnvironmentStringsW(src, None) } as usize;
    let mut out = vec![0u16; need];
    let got = unsafe { ExpandEnvironmentStringsW(src, Some(&mut out)) } as usize;
    // `got` counts the terminating zero; 0 (failure) leaves nothing.
    String::from_utf16_lossy(&out[..got.saturating_sub(1).min(need)])
}

fn rule_of(rule: &INetFwRule) -> Result<Rule> {
    unsafe {
        Ok(Rule {
            program: os("ApplicationName", rule.ApplicationName())?.to_string(),
            inbound: os("Direction", rule.Direction())? == NET_FW_RULE_DIR_IN,
            allow: os("Action", rule.Action())? == NET_FW_ACTION_ALLOW,
            enabled: os("Enabled", rule.Enabled())?.as_bool(),
            profiles: os("Profiles", rule.Profiles())?,
        })
    }
}

/// The rule an enumeration step produced, if it is one.
///
/// # Safety
/// `item` must be a VARIANT the enumeration filled in.
unsafe fn rule_in(item: &VARIANT) -> Option<INetFwRule> {
    let inner = unsafe { &item.Anonymous.Anonymous };
    let dispatch: Option<IDispatch> = (inner.vt == VT_DISPATCH)
        .then(|| unsafe { (*inner.Anonymous.pdispVal).clone() })
        .flatten();
    dispatch?.cast().ok()
}

/// Reads the firewall policy. Needs no elevation; works from any thread.
pub(crate) fn read_policy() -> Result<Policy> {
    let _com = Com::init();
    let policy: INetFwPolicy2 = os("NetFwPolicy2", unsafe {
        CoCreateInstance(&NetFwPolicy2, None, CLSCTX_ALL)
    })?;
    let current = os("CurrentProfileTypes", unsafe {
        policy.CurrentProfileTypes()
    })?;
    let mut enabled = 0;
    for profile in [
        NET_FW_PROFILE2_DOMAIN,
        NET_FW_PROFILE2_PRIVATE,
        NET_FW_PROFILE2_PUBLIC,
    ] {
        if os("FirewallEnabled", unsafe {
            policy.get_FirewallEnabled(profile)
        })?
        .as_bool()
        {
            enabled |= profile.0;
        }
    }
    let rules = os("Rules", unsafe { policy.Rules() })?;
    let walk: IEnumVARIANT = os(
        "Rules enumeration",
        unsafe { rules._NewEnum() }.and_then(|e| e.cast()),
    )?;
    let mut all = Vec::new();
    loop {
        let mut item = [VARIANT::default()];
        let mut fetched = 0;
        let step = unsafe { walk.Next(&mut item, &mut fetched) };
        if fetched == 0 {
            os("Rules enumeration", step.ok())?;
            break;
        }
        let [mut item] = item;
        let rule = unsafe { rule_in(&item) };
        let _ = unsafe { VariantClear(&mut item) };
        all.push(rule_of(&rule.ok_or(Error::Os("not a rule".into()))?)?);
    }
    Ok(Policy {
        enabled,
        current,
        rules: all,
    })
}

/// Whether Mouser is blocked right now; `None` when the policy cannot be read.
pub(crate) fn check_now(exe: &Path) -> Option<bool> {
    let policy = read_policy().ok()?;
    Some(is_blocked(&policy, exe, expand_variables))
}

/// Re-checks on every event and every `period`, and tells `emit` when the
/// answer changes; a check that fails (`None`) changes nothing. Ends when the
/// events end.
pub(crate) fn run(
    events: &Receiver<()>,
    period: Duration,
    mut blocked: impl FnMut() -> Option<bool>,
    emit: impl Fn(PlatformProblem),
) {
    let mut raised = false;
    loop {
        if let Some(now) = blocked()
            && now != raised
        {
            raised = now;
            emit(if now {
                PlatformProblem::Raised(BLOCKED.into())
            } else {
                PlatformProblem::Cleared(BLOCKED.into())
            });
        }
        match events.recv_timeout(period) {
            Ok(()) | Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return,
        }
        // A burst of changes is one re-check.
        while events.try_recv().is_ok() {}
    }
}

/// Starts watching for `exe`; dropping the handle stops it.
pub(crate) fn watch(
    exe: std::path::PathBuf,
    emit: impl Fn(PlatformProblem) + Send + 'static,
) -> Result<WatchHandle> {
    let (tx, rx) = channel();
    let network = netwatch::watch(tx)?;
    let thread = std::thread::spawn(move || run(&rx, RECHECK, || check_now(&exe), emit));
    Ok(WatchHandle::new(move || {
        // Dropping the network watch ends the events, which ends the thread.
        drop(network);
        thread.join().expect("firewall watch thread");
    }))
}
