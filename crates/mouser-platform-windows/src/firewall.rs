//! Windows Firewall detection (read-only, no elevation).
//!
//! The first time Mouser listens, Windows asks whether to allow it through the
//! firewall; cancelling that prompt leaves a Block rule behind, and a profile
//! that was never allowed leaves the app unreachable. Either way other
//! computers cannot connect to this PC, and nothing in Mouser says why. This
//! reads the firewall policy (`INetFwPolicy2`) and raises a problem while the
//! firewall is on for a current network profile and Mouser is not let in on it
//! (by a rule for its program, a rule for its UDP port, or a default of Allow).
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
    /// `NET_FW_IP_PROTOCOL_*`: 6 TCP, 17 UDP, 256 any.
    pub(crate) protocol: i32,
    /// The rule's local ports as written: `*`, `47801`, `1000,2000-3000`;
    /// empty means all.
    pub(crate) local_ports: String,
    /// The Windows service the rule is limited to; empty for none.
    pub(crate) service: String,
}

/// The firewall as it stands now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Policy {
    /// Profiles the firewall is switched on for.
    pub(crate) enabled: i32,
    /// Profiles of the networks this PC is connected to now.
    pub(crate) current: i32,
    /// Profiles whose default for inbound traffic is Allow.
    pub(crate) default_allow: i32,
    pub(crate) rules: Vec<Rule>,
}

/// The UDP port other computers connect to: `advanced.port`'s default
/// (`mouser-proto` config). The platform crate is not told the configured
/// port, and the seam does not carry it, so a changed port is not followed.
pub(crate) const PORT: u16 = 47801;

const UDP: i32 = 17;
const ANY_PROTOCOL: i32 = 256;

/// Whether a rule's `LocalPorts` text includes `port`: `*` or nothing (all),
/// numbers, comma lists and `from-to` ranges. Names Windows also accepts
/// (`RPC`, `Teredo`...) are not ports and match nothing.
pub(crate) fn ports_include(spec: &str, port: u16) -> bool {
    let spec = spec.trim();
    spec.is_empty()
        || spec.split(',').any(|part| {
            let part = part.trim();
            let range = part.split_once('-').unwrap_or((part, part));
            let number = |s: &str| s.trim().parse::<u16>().ok();
            part == "*" || matches!((number(range.0), number(range.1)), (Some(a), Some(b)) if (a..=b).contains(&port))
        })
}

/// Paths compare without regard to case or the kind of slash.
fn normal(path: &str) -> String {
    path.trim()
        .trim_matches('"')
        .replace('/', "\\")
        .to_lowercase()
}

/// Whether Mouser (`exe`, listening on UDP `port`) is kept out. For each
/// network profile that is both current and has the firewall on:
/// - an enabled inbound Block rule naming Mouser's program blocks it (Block
///   wins in Windows), whatever else holds;
/// - otherwise it is let in if the profile's default for inbound is Allow, or
///   an enabled inbound Allow rule names Mouser's program, or an enabled
///   inbound Allow rule for no program in particular covers UDP `port`
///   (protocol UDP or any, `LocalPorts` including the port);
/// - otherwise it is blocked.
///
/// A rule must cover the profile (its `Profiles` mask). A rule limited to a
/// Windows service is not counted as letting Mouser in, since it is about
/// that service's traffic; other restrictions (remote addresses, interface
/// types) are not looked at, as they still let other computers in. Block
/// rules that name no program (a port, a service) are ignored.
/// `expand` turns `%VARIABLES%` in a rule's program path into their values.
pub(crate) fn is_blocked(
    policy: &Policy,
    exe: &Path,
    port: u16,
    expand: impl Fn(&str) -> String,
) -> bool {
    let exe = normal(&exe.to_string_lossy());
    let inbound: Vec<&Rule> = policy
        .rules
        .iter()
        .filter(|r| r.enabled && r.inbound)
        .collect();
    let named = |r: &&&Rule| normal(&expand(&r.program)) == exe;
    let anywhere = |r: &&&Rule| {
        let any = r.program.trim().is_empty() || r.program.trim().eq_ignore_ascii_case("any");
        any && r.service.is_empty()
            && [UDP, ANY_PROTOCOL].contains(&r.protocol)
            && ports_include(&r.local_ports, port)
    };
    let mine: Vec<&Rule> = inbound.iter().filter(named).copied().collect();
    let open: Vec<&Rule> = inbound.iter().filter(anywhere).copied().collect();
    [
        NET_FW_PROFILE2_DOMAIN,
        NET_FW_PROFILE2_PRIVATE,
        NET_FW_PROFILE2_PUBLIC,
    ]
    .iter()
    .map(|p| p.0)
    .filter(|p| policy.current & policy.enabled & p != 0)
    .any(|p| {
        let allows = |rules: &[&Rule], allow: bool| {
            rules
                .iter()
                .any(|r| r.profiles & p != 0 && r.allow == allow)
        };
        let let_in = policy.default_allow & p != 0 || allows(&mine, true) || allows(&open, true);
        allows(&mine, false) || !let_in
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
            protocol: os("Protocol", rule.Protocol())?,
            local_ports: os("LocalPorts", rule.LocalPorts())?.to_string(),
            service: os("ServiceName", rule.ServiceName())?.to_string(),
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
    let made = unsafe { CoCreateInstance(&NetFwPolicy2, None, CLSCTX_ALL) };
    let policy: INetFwPolicy2 = os("NetFwPolicy2", made)?;
    let current = os("CurrentProfileTypes", unsafe {
        policy.CurrentProfileTypes()
    })?;
    let (mut enabled, mut default_allow) = (0, 0);
    for profile in [
        NET_FW_PROFILE2_DOMAIN,
        NET_FW_PROFILE2_PRIVATE,
        NET_FW_PROFILE2_PUBLIC,
    ] {
        let on = unsafe { policy.get_FirewallEnabled(profile) };
        let default = unsafe { policy.get_DefaultInboundAction(profile) };
        enabled |= profile.0 * i32::from(os("FirewallEnabled", on)?.as_bool());
        default_allow |=
            profile.0 * i32::from(os("DefaultInboundAction", default)? == NET_FW_ACTION_ALLOW);
    }
    let rules = os("Rules", unsafe { policy.Rules() })?;
    let walk = unsafe { rules._NewEnum() }.and_then(|e| e.cast::<IEnumVARIANT>());
    let walk = os("Rules enumeration", walk)?;
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
        default_allow,
        rules: all,
    })
}

/// Whether Mouser is blocked right now; `None` when the policy cannot be read.
pub(crate) fn check_now(exe: &Path, port: u16) -> Option<bool> {
    let policy = read_policy().ok()?;
    Some(is_blocked(&policy, exe, port, expand_variables))
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
    let thread = std::thread::spawn(move || run(&rx, RECHECK, || check_now(&exe, PORT), emit));
    Ok(WatchHandle::new(move || {
        // Dropping the network watch ends the events, which ends the thread.
        drop(network);
        thread.join().expect("firewall watch thread");
    }))
}
