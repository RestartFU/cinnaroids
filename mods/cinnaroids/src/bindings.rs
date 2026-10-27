//! Resolves this mod's policies from physical observations and the player's own chords.

use mod_api::bindings::cinnabar::extension::input::{
    self, BindingContext, BindingModifiers, Bindings, Controls,
};
use std::collections::{BTreeMap, VecDeque};

/// Binding decoding, indexing and retirement each advance by at most this many rows per frame.
pub const BINDINGS_PER_FRAME: u32 = 8;
const _: () = assert!(BINDINGS_PER_FRAME as usize <= mod_api::MAX_INPUT_BINDINGS_PER_READ);

/// Chords sharing a physical key, compressed into the sixteen possible modifier masks.
#[derive(Default)]
struct Key {
    all: u16,
    attack: u16,
    jump: u16,
}

#[derive(Default)]
pub struct Cache {
    revision: Option<u64>,
    offset: u32,
    total: u32,
    complete: bool,
    has_attack: bool,
    has_jump: bool,
    keys: BTreeMap<String, Key>,
    retired: VecDeque<BTreeMap<String, Key>>,
}

impl Cache {
    /// Replaces stale assignments without freeing a whole binding index during one callback.
    fn invalidate(&mut self) {
        if !self.keys.is_empty() {
            self.retired.push_back(std::mem::take(&mut self.keys));
        }
        self.revision = None;
        self.offset = 0;
        self.total = 0;
        self.complete = false;
        self.has_attack = false;
        self.has_jump = false;
    }

    /// Retires no more rows than a read can add, keeping memory and cleanup work bounded.
    fn retire(&mut self) {
        for _ in 0..BINDINGS_PER_FRAME {
            let Some(old) = self.retired.front_mut() else {
                break;
            };
            old.pop_first();
            if old.is_empty() {
                self.retired.pop_front();
            }
        }
    }

    /// Fetches a bounded page; no action policy observes a partially collected revision.
    pub fn refresh(
        &mut self,
        read: impl FnOnce(Option<u64>, u32, u32) -> Result<Option<Bindings>, String>,
    ) {
        self.retire();
        let offset = if self.complete { 0 } else { self.offset };
        let after = self.complete.then_some(self.revision).flatten();
        match read(after, offset, BINDINGS_PER_FRAME) {
            Ok(Some(page)) => {
                if page.total as usize > mod_api::MAX_INPUT_BINDINGS
                    || offset > page.total
                    || page.entries.len() > BINDINGS_PER_FRAME as usize
                    || page.entries.len() as u32 > page.total - offset
                    || page.entries.is_empty() && offset < page.total
                {
                    self.invalidate();
                    return;
                }
                if self.revision != Some(page.revision) {
                    self.invalidate();
                    self.revision = Some(page.revision);
                    self.total = page.total;
                    if offset != 0 {
                        return;
                    }
                }
                if page.total != self.total || offset != self.offset {
                    self.invalidate();
                    return;
                }
                self.complete = false;
                self.offset += page.entries.len() as u32;
                for binding in page.entries {
                    if binding.context != BindingContext::Gameplay {
                        continue;
                    }
                    let bit = 1 << modifier_mask(&binding.modifiers);
                    let key = self.keys.entry(binding.key).or_default();
                    key.all |= bit;
                    match binding.action.as_str() {
                        "attack" => {
                            key.attack |= bit;
                            self.has_attack = true;
                        }
                        "jump" => {
                            key.jump |= bit;
                            self.has_jump = true;
                        }
                        _ => {}
                    }
                }
                self.complete = self.offset == self.total;
            }
            Ok(None) if self.complete => {}
            _ => self.invalidate(),
        }
    }

    /// Whether a complete, current player assignment set is available to action policy.
    pub fn ready(&self) -> bool {
        self.complete
    }

    /// Resolves both policies once from raw held keys and complete player assignments.
    pub fn resolve(&self, controls: &Controls, reserved: &[String]) -> Holds {
        let mut result = Holds::default();
        if !(self.complete && controls.focused && controls.gameplay && !controls.panel_open)
            || !(self.has_attack || self.has_jump)
        {
            return result;
        }
        let actual = physical_modifiers(&controls.keys_held, reserved);
        let satisfied = (0u8..16)
            .filter(|mask| mask & actual == *mask)
            .fold(0u16, |bits, mask| bits | (1 << mask));
        for (name, key) in &self.keys {
            if key.attack | key.jump == 0 {
                continue;
            }
            let mut active = key.all & satisfied;
            let mut maximum = 0;
            let mut winning = 0;
            while active != 0 {
                let mask = active.trailing_zeros();
                let bit = 1 << mask;
                active &= active - 1;
                let specificity = mask.count_ones();
                if specificity > maximum {
                    maximum = specificity;
                    winning = 0;
                }
                if specificity == maximum {
                    winning |= bit;
                }
            }
            let attack = winning & key.attack != 0;
            let jump = winning & key.jump != 0;
            if !(attack || jump) || reserved.contains(name) || !controls.keys_held.contains(name) {
                continue;
            }
            result.attack |= attack;
            result.jump |= jump;
            if (result.attack || !self.has_attack) && (result.jump || !self.has_jump) {
                break;
            }
        }
        result
    }

    /// Selects one guest-owned policy result for focused unit coverage.
    #[cfg(test)]
    fn held(&self, action: &str, controls: &Controls, reserved: &[String]) -> bool {
        let holds = self.resolve(controls, reserved);
        match action {
            "attack" => holds.attack,
            "jump" => holds.jump,
            _ => false,
        }
    }

    /// Polls unchanged revisions cheaply and advances changed assignments one page per frame.
    pub fn read(&mut self) {
        self.refresh(input::read_bindings);
    }
}

/// A chord's four required modifier groups, in the same order as physical sampling.
fn modifier_mask(required: &BindingModifiers) -> u8 {
    u8::from(required.shift)
        | (u8::from(required.control) << 1)
        | (u8::from(required.alt) << 2)
        | (u8::from(required.super_key) << 3)
}

/// Either physical side satisfies a modifier group; reserved keys cannot satisfy gameplay.
fn physical_modifiers(held: &[String], reserved: &[String]) -> u8 {
    [
        ("ShiftLeft", "ShiftRight"),
        ("ControlLeft", "ControlRight"),
        ("AltLeft", "AltRight"),
        ("SuperLeft", "SuperRight"),
    ]
    .into_iter()
    .enumerate()
    .fold(0, |mask, (index, (left, right))| {
        mask | (u8::from(
            held.iter()
                .any(|key| (key == left || key == right) && !reserved.contains(key)),
        ) << index)
    })
}

/// Action holds computed entirely inside this mod.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Holds {
    pub attack: bool,
    pub jump: bool,
}

#[cfg(test)]
mod tests;
