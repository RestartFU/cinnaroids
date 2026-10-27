//! Resolves this mod's policies from physical observations and the player's own chords.

use mod_api::bindings::cinnabar::extension::input::{
    self, BindingContext, BindingModifiers, Bindings, Controls,
};
use std::collections::VecDeque;

/// Binding decoding, indexing and retirement each advance by at most this many rows per frame.
pub const BINDINGS_PER_FRAME: u32 = 8;
const _: () = assert!(BINDINGS_PER_FRAME as usize <= mod_api::MAX_INPUT_BINDINGS_PER_READ);

/// Lossless bounded key names make ordered lookups compare words instead of scanning strings.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
struct KeyName {
    words: [u64; mod_api::MAX_CONTROL_KEY_BYTES.div_ceil(8)],
    length: usize,
}

impl KeyName {
    /// Packs every byte and the length, bounded by the input name limit.
    fn new(name: &str) -> Option<Self> {
        if name.is_empty() || name.len() > mod_api::MAX_CONTROL_KEY_BYTES {
            return None;
        }
        let mut result = Self {
            words: [0; mod_api::MAX_CONTROL_KEY_BYTES.div_ceil(8)],
            length: name.len(),
        };
        for (word, bytes) in result.words.iter_mut().zip(name.as_bytes().chunks(8)) {
            let mut padded = [0; 8];
            padded[..bytes.len()].copy_from_slice(bytes);
            *word = u64::from_le_bytes(padded);
        }
        Some(result)
    }
}

/// Rejects unrelated names cheaply; collisions still require the exact indexed lookup.
#[derive(Default)]
struct KeyFilter([u64; 4]);

impl KeyFilter {
    /// Samples bounded name bytes without allocating or packing the full name.
    fn bit(name: &str) -> (usize, u64) {
        let bytes = name.as_bytes();
        let first = bytes.first().copied().unwrap_or(0);
        let last = bytes.last().copied().unwrap_or(0);
        let hash = first.wrapping_mul(31) ^ last.wrapping_mul(7) ^ bytes.len() as u8;
        (usize::from(hash / 64), 1 << (hash % 64))
    }

    /// Adds a key for one action, irrespective of its required modifiers.
    fn insert(&mut self, name: &str) {
        let (word, bit) = Self::bit(name);
        self.0[word] |= bit;
    }

    /// A missing bit proves the action has no chord for this key.
    fn contains(&self, (word, bit): (usize, u64)) -> bool {
        self.0[word] & bit != 0
    }
}

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
    attack_keys: KeyFilter,
    jump_keys: KeyFilter,
    // Sorted for binary lookup; insertion moves at most MAX_INPUT_BINDINGS bounded rows.
    keys: Vec<(KeyName, Key)>,
    retired: VecDeque<Vec<(KeyName, Key)>>,
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
        self.attack_keys = KeyFilter::default();
        self.jump_keys = KeyFilter::default();
    }

    /// Retires no more rows than a read can add, keeping memory and cleanup work bounded.
    fn retire(&mut self) {
        for _ in 0..BINDINGS_PER_FRAME {
            let Some(old) = self.retired.front_mut() else {
                break;
            };
            old.pop();
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
                    let Some(name) = KeyName::new(&binding.key) else {
                        continue;
                    };
                    let bit = 1 << modifier_mask(&binding.modifiers);
                    let index = match self.keys.binary_search_by(|(stored, _)| stored.cmp(&name)) {
                        Ok(index) => index,
                        Err(index) => {
                            self.keys.insert(index, (name, Key::default()));
                            index
                        }
                    };
                    let key = &mut self.keys[index].1;
                    key.all |= bit;
                    match binding.action.as_str() {
                        "attack" => {
                            key.attack |= bit;
                            self.has_attack = true;
                            self.attack_keys.insert(&binding.key);
                        }
                        "jump" => {
                            key.jump |= bit;
                            self.has_jump = true;
                            self.jump_keys.insert(&binding.key);
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

    /// Resolves requested policies in one pass over admitted keys and complete assignments.
    pub fn resolve(&self, controls: &Controls, needed: Holds) -> Holds {
        let mut needed = Holds {
            attack: needed.attack && self.has_attack,
            jump: needed.jump && self.has_jump,
        };
        let mut result = Holds::default();
        if !(self.complete && controls.focused && controls.gameplay && !controls.panel_open)
            || !(needed.attack || needed.jump)
        {
            return result;
        }
        let actual = physical_modifiers(&controls.keys_admitted);
        let satisfied = (0u8..16)
            .filter(|mask| mask & actual == *mask)
            .fold(0u16, |bits, mask| bits | (1 << mask));
        let mut candidates = self.keys_for(needed);
        for name in &controls.keys_admitted {
            if !candidates.contains(KeyFilter::bit(name)) {
                continue;
            }
            let Some(id) = KeyName::new(name) else {
                continue;
            };
            let Ok(index) = self.keys.binary_search_by(|(stored, _)| stored.cmp(&id)) else {
                continue;
            };
            let key = &self.keys[index].1;
            if (key.attack == 0 || !needed.attack) && (key.jump == 0 || !needed.jump) {
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
            let attack = needed.attack && winning & key.attack != 0;
            let jump = needed.jump && winning & key.jump != 0;
            result.attack |= attack;
            result.jump |= jump;
            needed.attack &= !attack;
            needed.jump &= !jump;
            if !(needed.attack || needed.jump) {
                break;
            }
            if attack || jump {
                candidates = self.keys_for(needed);
            }
        }
        result
    }

    /// Combines only unresolved actions so later keys need one cheap filter check.
    fn keys_for(&self, needed: Holds) -> KeyFilter {
        KeyFilter(std::array::from_fn(|index| {
            (if needed.attack {
                self.attack_keys.0[index]
            } else {
                0
            }) | (if needed.jump {
                self.jump_keys.0[index]
            } else {
                0
            })
        }))
    }

    /// Selects one guest-owned policy result for focused unit coverage.
    #[cfg(test)]
    fn held(&self, action: &str, controls: &Controls) -> bool {
        let holds = self.resolve(
            controls,
            Holds {
                attack: action == "attack",
                jump: action == "jump",
            },
        );
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

/// Either admitted physical side satisfies a modifier group.
fn physical_modifiers(held: &[String]) -> u8 {
    held.iter().fold(0, |mask, key| {
        let bit = match key.as_str() {
            "ShiftLeft" | "ShiftRight" => 1,
            "ControlLeft" | "ControlRight" => 2,
            "AltLeft" | "AltRight" => 4,
            "SuperLeft" | "SuperRight" => 8,
            _ => return mask,
        };
        mask | bit
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
