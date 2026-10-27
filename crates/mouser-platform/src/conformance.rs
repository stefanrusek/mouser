//! Behavioural-rule checks 1-11 of contracts/platform-api.md.
//!
//! Every backend's `contract_platform_<os>.rs` (and the testkit's
//! `contract_platform_virtual.rs`) calls the same checks. Each check panics on
//! violation. A capability that returns `Err(Unsupported)` is skipped by the
//! rules that need it; rule 9 verifies it is declared as a limitation.
//!
//! What a check can observe is bounded by the trait: swallowing of local input
//! while grabbed (rule 1) and release of held keys on drop (rule 2) are
//! exercised here for errors only; a backend's own contract test observes the
//! OS side, and `VirtualPlatform` asserts them through its recorders.

use crate::{Capability, ClipRep, Error, InterfaceKind, LocalInput, Platform, Result};
use std::net::IpAddr;
use std::sync::mpsc::{RecvTimeoutError, channel};
use std::time::Duration;

/// Name prefixes of interfaces that are never a physical LAN (research R3).
const NON_LAN_PREFIXES: &[&str] = &[
    "lo",
    "utun",
    "ipsec",
    "ppp",
    "tun",
    "tap",
    "wg",
    "tailscale",
    "zt",
    "gif",
    "stf",
    "bridge",
    "awdl",
    "llw",
    "anpi",
    "ap",
    "vmnet",
    "vboxnet",
    "docker",
    "br-",
    "virbr",
    "veth",
    "vEthernet",
];

/// Every check, in rule order, for backends that run them all.
pub type Check = fn(&dyn Platform);

pub const ALL: &[(&str, Check)] = &[
    ("rule 1: grab", rule_1_grab),
    ("rule 2: injected input is releasable", rule_2_release),
    ("rule 3: key by HID usage", rule_3_hid_usage),
    ("rule 4: display scale and size", rule_4_displays),
    ("rule 5: clipboard round trip", rule_5_clipboard),
    ("rule 6: interface classification", rule_6_interfaces),
    ("rule 7: no self-capture", rule_7_no_self_capture),
    ("rule 8: secrets persist", rule_8_secrets),
    ("rule 9: unsupported is declared", rule_9_limitations),
];

/// `None` when the capability is unsupported; any other error crashes loudly.
fn supported<T>(r: Result<T>) -> Option<T> {
    match r {
        Err(Error::Unsupported) => None,
        other => Some(other.unwrap()),
    }
}

/// Rule 1: `set_grabbed(true)` then `set_grabbed(false)` are accepted while
/// capturing, with the pointer warped between them. A platform that cannot
/// force a grab (Wayland) refuses both with `Unsupported`.
pub fn rule_1_grab(p: &dyn Platform) {
    let Some(mut capture) = supported(p.capture()) else {
        return;
    };
    let (tx, _rx) = channel();
    capture.start(tx).unwrap();
    if supported(capture.set_grabbed(true)).is_some() {
        capture.warp(10.0, 10.0).unwrap();
        capture.set_grabbed(false).unwrap();
    } else {
        assert!(
            matches!(capture.set_grabbed(false), Err(Error::Unsupported)),
            "a platform that cannot grab cannot release a grab either"
        );
    }
    capture.stop();
}

/// Rule 2: every key/button down is releasable, and dropping the injector with
/// input still held is accepted.
pub fn rule_2_release(p: &dyn Platform) {
    let Some(mut inject) = supported(p.inject()) else {
        return;
    };
    inject.key(0x0007_0004, true).unwrap();
    inject.key(0x0007_0004, false).unwrap();
    inject.button(1, true).unwrap();
    inject.button(1, false).unwrap();
    inject.key(0x0007_0005, true).unwrap();
    inject.button(2, true).unwrap();
    drop(inject);
}

/// Rule 3: keys are addressed by HID usage `(page << 16) | id`; letters, Enter,
/// modifiers and a consumer-page key (Volume Up) are
/// accepted as down/up pairs.
pub fn rule_3_hid_usage(p: &dyn Platform) {
    let Some(mut inject) = supported(p.inject()) else {
        return;
    };
    for usage in [
        0x0007_0004,
        0x0007_001D,
        0x0007_0028,
        0x0007_00E0,
        0x0007_00E3,
        0x000C_00E9,
    ] {
        inject.key(usage, true).unwrap();
        inject.key(usage, false).unwrap();
    }
}

/// Rule 4: every display reports a scale of at least 1.0, a logical size equal
/// to pixel size divided by scale, and a positive physical size when known.
pub fn rule_4_displays(p: &dyn Platform) {
    let displays = p.displays().unwrap();
    assert!(!displays.is_empty(), "no displays reported");
    for d in displays {
        assert!(d.scale >= 1.0, "scale below 1.0 on {}", d.name);
        let scale = f64::from(d.scale);
        assert!((d.logical_size.0 - f64::from(d.pixel_size.0) / scale).abs() < 1.0);
        assert!((d.logical_size.1 - f64::from(d.pixel_size.1) / scale).abs() < 1.0);
        if let Some((w, h)) = d.physical_size_mm {
            assert!(w > 0.0 && h > 0.0, "non-positive physical size");
        }
    }
}

/// Rule 5: Text, Html, Rtf and Png written together are all read back, and the
/// change counter advances.
pub fn rule_5_clipboard(p: &dyn Platform) {
    let Some(mut clipboard) = supported(p.clipboard()) else {
        return;
    };
    let reps = vec![
        ClipRep::Text("mouser conformance".into()),
        ClipRep::Html("<b>mouser conformance</b>".into()),
        ClipRep::Rtf(b"{\\rtf1 mouser}".to_vec()),
        ClipRep::Png(vec![0x89, b'P', b'N', b'G']),
    ];
    let before = clipboard.change_count();
    clipboard.write(reps.clone()).unwrap();
    assert!(
        clipboard.change_count() > before,
        "change count did not advance"
    );
    let read = clipboard.read().unwrap();
    for rep in reps {
        assert!(read.contains(&rep), "representation lost: {rep:?}");
    }
}

/// Rule 6: no VPN, VM, container, bridge, AWDL or loopback interface is
/// `PhysicalLan`, and nothing named like one is.
pub fn rule_6_interfaces(p: &dyn Platform) {
    for iface in p.interfaces() {
        if iface.kind != InterfaceKind::PhysicalLan {
            continue;
        }
        let virtual_name = NON_LAN_PREFIXES.iter().any(|x| iface.name.starts_with(x));
        assert!(!virtual_name, "{} classified PhysicalLan", iface.name);
        let loopback = iface.addrs.iter().any(IpAddr::is_loopback);
        assert!(!loopback, "{} has a loopback address", iface.name);
    }
}

/// Rule 7: input injected by this platform is never delivered back to its own
/// capture.
pub fn rule_7_no_self_capture(p: &dyn Platform) {
    let (Some(mut capture), Some(mut inject)) = (supported(p.capture()), supported(p.inject()))
    else {
        return;
    };
    let (tx, rx) = channel::<LocalInput>();
    capture.start(tx).unwrap();
    inject.motion_abs(20.0, 20.0).unwrap();
    inject.button(1, true).unwrap();
    inject.button(1, false).unwrap();
    inject.scroll(0.0, 1.0).unwrap();
    inject.key(0x0007_0073, true).unwrap();
    inject.key(0x0007_0073, false).unwrap();
    let got = rx.recv_timeout(Duration::from_millis(200));
    assert_eq!(
        got,
        Err(RecvTimeoutError::Timeout),
        "injected input was captured"
    );
    capture.stop();
}

/// Rule 8: a secret written to one store handle is read back unchanged from a
/// freshly opened one, and deleted secrets are gone.
pub fn rule_8_secrets(p: &dyn Platform) {
    const NAME: &str = "mouser-conformance-secret";
    let mut first = p.secrets();
    first.set(NAME, &[1, 2, 3, 255]).unwrap();
    drop(first);
    let mut reopened = p.secrets();
    assert_eq!(reopened.get(NAME).unwrap(), Some(vec![1, 2, 3, 255]));
    reopened.delete(NAME).unwrap();
    assert_eq!(reopened.get(NAME).unwrap(), None);
}

/// Rule 9: every capability that returns `Err(Unsupported)` (or `None` for file
/// drag) is listed in `PlatformInfo.limitations` with plain-language text.
pub fn rule_9_limitations(p: &dyn Platform) {
    let info = p.info();
    for l in &info.limitations {
        assert!(
            !l.text.trim().is_empty(),
            "blank limitation text for {:?}",
            l.capability
        );
    }
    let declared = |c: Capability| info.limitations.iter().any(|l| l.capability == c);
    let mut unsupported = Vec::new();
    if supported(p.capture()).is_none() {
        unsupported.push(Capability::Capture);
    }
    match supported(p.inject()) {
        None => unsupported.push(Capability::Inject),
        Some(mut inject) => {
            if supported(inject.secure_attention()).is_none() {
                unsupported.push(Capability::SecureAttention);
            }
        }
    }
    if supported(p.clipboard()).is_none() {
        unsupported.push(Capability::Clipboard);
    }
    if p.file_drag().is_none() {
        unsupported.push(Capability::FileDrag);
    }
    for c in unsupported {
        assert!(
            declared(c),
            "{c:?} is unsupported but not listed as a limitation"
        );
    }
}

/// Rule 10: after `stop()`, and after the capture is dropped while grabbed,
/// the pointer is visible. `pointer_hidden` is the backend's own query (on
/// `VirtualPlatform`, its recorded grab state). A platform that cannot grab
/// has nothing to hide.
pub fn rule_10_pointer_visible(p: &dyn Platform, pointer_hidden: &dyn Fn() -> bool) {
    let Some(mut capture) = supported(p.capture()) else {
        return;
    };
    let (tx, _rx) = channel();
    capture.start(tx).unwrap();
    if supported(capture.set_grabbed(true)).is_none() {
        capture.stop();
        return;
    }
    capture.stop();
    assert!(!pointer_hidden(), "pointer hidden after stop");

    let (tx, _rx) = channel();
    capture.start(tx).unwrap();
    capture.set_grabbed(true).unwrap();
    drop(capture);
    assert!(!pointer_hidden(), "pointer hidden after drop while grabbed");
}

/// Rule 11: when the OS refuses, inject and capture methods return `Err` and
/// never panic. A button above 5 returns `Err(Os(_))` or `Ok(())`; `warp`
/// outside every display returns `Err`.
pub fn rule_11_refusal_is_error(p: &dyn Platform) {
    if let Some(mut inject) = supported(p.inject()) {
        let r = inject.button(6, true);
        let acceptable = matches!(r, Ok(()) | Err(Error::Os(_)));
        assert!(acceptable, "button above 5 returned {r:?}");
    }
    let Some(mut capture) = supported(p.capture()) else {
        return;
    };
    let (tx, _rx) = channel();
    capture.start(tx).unwrap();
    let far = p
        .displays()
        .unwrap()
        .iter()
        .map(|d| d.origin.0 + d.logical_size.0)
        .fold(0.0, f64::max)
        + 1_000_000.0;
    assert!(
        capture.warp(far, far).is_err(),
        "warp outside every display succeeded"
    );
    capture.stop();
}
