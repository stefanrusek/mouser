//! Aggregated double-usage signal (D2): at process exit one banner lists every
//! test that ran against `VirtualPlatform` or `SimNet` in place of real
//! configuration.

use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;
use std::sync::{Mutex, MutexGuard, Once};

type Registry = BTreeMap<String, BTreeSet<&'static str>>;

static REGISTRY: Mutex<Registry> = Mutex::new(BTreeMap::new());
static INSTALL: Once = Once::new();

unsafe extern "C" {
    fn atexit(callback: extern "C" fn()) -> i32;
}

extern "C" fn print_at_exit() {
    if let Some(banner) = banner() {
        let _ = std::io::stderr().write_all(banner.as_bytes());
    }
}

fn registry() -> MutexGuard<'static, Registry> {
    REGISTRY
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Record that the current test (named after its thread) used `double`, and
/// arrange for the banner to print at process exit.
pub fn note(double: &'static str) {
    INSTALL.call_once(|| {
        // SAFETY: `print_at_exit` is a plain `extern "C"` fn that never unwinds.
        let rc = unsafe { atexit(print_at_exit) };
        assert_eq!(rc, 0, "atexit registration failed");
    });
    let thread = std::thread::current();
    let test = thread.name().unwrap_or("<unnamed thread>").to_string();
    registry().entry(test).or_default().insert(double);
}

/// The banner text, or `None` when no double was used in this process.
pub fn banner() -> Option<String> {
    let reg = registry();
    if reg.is_empty() {
        return None;
    }
    let mut out = String::from(
        "\n==== TEST DOUBLES IN USE (decision D2) ====\n\
         These tests ran against simulated infrastructure, not real configuration:\n",
    );
    for (test, doubles) in reg.iter() {
        let names: Vec<&str> = doubles.iter().copied().collect();
        out.push_str(&format!("  {test}  [{}]\n", names.join(", ")));
    }
    out.push_str("============================================\n");
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn banner_lists_each_test_with_its_doubles() {
        note("VirtualPlatform");
        note("SimNet");
        note("SimNet");
        let banner = banner().expect("banner after note");
        assert!(banner.contains("TEST DOUBLES IN USE"));
        let line = banner
            .lines()
            .find(|l| l.contains("banner_lists_each_test_with_its_doubles"))
            .expect("this test is listed");
        assert!(line.ends_with("[SimNet, VirtualPlatform]"), "{line}");
    }

    #[test]
    fn unnamed_threads_are_still_listed() {
        std::thread::Builder::new()
            .spawn(|| note("SimNet"))
            .unwrap()
            .join()
            .unwrap();
        assert!(banner().unwrap().contains("<unnamed thread>"));
    }
}
