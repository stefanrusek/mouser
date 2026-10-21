//! Targeted paste (Story 6 scenario 3) and the shortcuts that trigger
//! clipboard work: the targeted-paste hotkey and the paste shortcut.
//!
//! Targeted paste fetches the latest remote content from the computer it was
//! copied on and writes it on this computer only: the write is tagged as
//! Mouser's own, so no other computer is told about it.

use std::collections::BTreeSet;

use mouser_proto::model::Os;
use mouser_proto::wire::Modifiers;

use super::{Clip, Out};
use crate::crossing::modifiers::held_modifiers;
use crate::crossing::panic::usage_for_name;

/// A hotkey: modifiers held, and the key pressed while they are.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Chord {
    modifiers: Modifiers,
    usage: u32,
}

impl Chord {
    /// `"Ctrl+Alt+Shift+V"`: any of `Ctrl`, `Alt`, `Shift`, `Meta` (or
    /// `Cmd`), then a key name as accepted by `hotkeys.panic`.
    pub fn parse(text: &str) -> Result<Self, String> {
        let mut names: Vec<&str> = text.split('+').collect();
        let key = names.pop().expect("split yields at least one part");
        let usage = usage_for_name(key).ok_or_else(|| format!("{key:?} is not a known key"))?;
        let mut modifiers = Modifiers::default();
        for name in names {
            match name {
                "Ctrl" => modifiers.ctrl = true,
                "Alt" => modifiers.alt = true,
                "Shift" => modifiers.shift = true,
                "Meta" | "Cmd" => modifiers.meta = true,
                other => {
                    return Err(format!(
                        "{other:?} is not a modifier (Ctrl, Alt, Shift, Meta)"
                    ));
                }
            }
        }
        Ok(Self { modifiers, usage })
    }

    /// `usage` went down while `held` were down.
    pub fn pressed(&self, held: &BTreeSet<u32>, usage: u32) -> bool {
        usage == self.usage && held_modifiers(held) == self.modifiers
    }
}

/// The paste shortcut of an operating system: Command-V on a Mac, Control-V
/// elsewhere.
pub fn paste_chord(os: Os) -> Chord {
    let key = usage_for_name("V").expect("V is a key");
    let mut modifiers = Modifiers::default();
    match os {
        Os::Macos => modifiers.meta = true,
        Os::Windows | Os::Linux => modifiers.ctrl = true,
    }
    Chord {
        modifiers,
        usage: key,
    }
}

impl Clip {
    /// The targeted-paste hotkey: fetch all of the latest remote content.
    pub fn targeted(&mut self) -> Vec<Out> {
        let Some(remote) = &self.remote else {
            return Vec::new();
        };
        let kinds = remote.kinds();
        self.begin(kinds, Vec::new())
    }
}
