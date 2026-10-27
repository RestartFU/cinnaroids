//! Resolves this mod's policies from physical observations and the player's own chords.

use mod_api::bindings::cinnabar::extension::input::{
    self, BindingContext, BindingModifiers, Bindings, Controls,
};
use std::collections::VecDeque;

/// Binding decoding, indexing and retirement each advance by at most this many rows per frame.
pub const BINDINGS_PER_FRAME: u32 = 4;
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

/// Chords sharing a physical key, compressed into the sixteen possible modifier masks.
#[derive(Default)]
struct Key {
    winning_attack: u16,
    winning_jump: u16,
    all: u16,
    attack: u16,
    jump: u16,
}

impl Key {
    /// Compiles exact specificity arbitration for all sixteen admitted modifier states.
    /// Unrequested actions participate in arbitration just as requested actions do.
    fn compile(&mut self) {
        self.winning_attack = 0;
        self.winning_jump = 0;
        for (actual, satisfied) in SATISFIED.iter().enumerate() {
            let active = self.all & satisfied;
            let winning = [0x8000, 0x6880, 0x1668, 0x0116, 0x0001]
                .into_iter()
                .map(|level| active & level)
                .find(|bits| *bits != 0)
                .unwrap_or(0);
            if winning & self.attack != 0 {
                self.winning_attack |= 1 << actual;
            }
            if winning & self.jump != 0 {
                self.winning_jump |= 1 << actual;
            }
        }
    }
}

/// Satisfied chord masks for each possible admitted modifier state.
const SATISFIED: [u16; 16] = {
    let mut table = [0; 16];
    let mut actual = 0;
    while actual < 16 {
        let mut required = 0;
        while required < 16 {
            if required & actual == required {
                table[actual] |= 1 << required;
            }
            required += 1;
        }
        actual += 1;
    }
    table
};

#[derive(Default)]
pub struct Cache {
    revision: Option<u64>,
    offset: u32,
    total: u32,
    complete: bool,
    // Sorted for lossless lookup of names added by future hosts.
    keys: Vec<(KeyName, Key)>,
    retired: VecDeque<Vec<(KeyName, Key)>>,
    native_attack: [[u64; 2]; 16],
    native_jump: [[u64; 2]; 16],
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
        self.native_attack = [[0; 2]; 16];
        self.native_jump = [[0; 2]; 16];
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
                        }
                        "jump" => {
                            key.jump |= bit;
                        }
                        _ => {}
                    }
                    key.compile();
                    if let Some(id) = native_key(&binding.key) {
                        let bit = 1 << (id % 64);
                        for actual in 0..16 {
                            let attack = &mut self.native_attack[actual][id / 64];
                            *attack = (*attack & !bit)
                                | if key.winning_attack & (1 << actual) != 0 {
                                    bit
                                } else {
                                    0
                                };
                            let jump = &mut self.native_jump[actual][id / 64];
                            *jump = (*jump & !bit)
                                | if key.winning_jump & (1 << actual) != 0 {
                                    bit
                                } else {
                                    0
                                };
                        }
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

    /// Compiled winners and bounded key decisions avoid chord arbitration and nested scans.
    pub fn resolve(&self, controls: &Controls, needed: Holds) -> Holds {
        let mut result = Holds::default();
        if !(self.complete && controls.focused && controls.gameplay && !controls.panel_open)
            || !(needed.attack || needed.jump)
        {
            return result;
        }
        let mut admitted = [0u64; 2];
        let mut future_names = false;
        for name in &controls.keys_admitted {
            if let Some(id) = native_key(name) {
                admitted[id / 64] |= 1 << (id % 64);
            } else {
                future_names = true;
            }
        }
        let modifiers = admitted[1] & 255;
        let actual = usize::from(modifiers & 3 != 0)
            | (usize::from(modifiers & 12 != 0) << 1)
            | (usize::from(modifiers & 48 != 0) << 2)
            | (usize::from(modifiers & 192 != 0) << 3);
        let intersects = |keys: [u64; 2]| (admitted[0] & keys[0]) | (admitted[1] & keys[1]) != 0;
        result.attack = needed.attack && intersects(self.native_attack[actual]);
        result.jump = needed.jump && intersects(self.native_jump[actual]);
        if future_names {
            for name in &controls.keys_admitted {
                if native_key(name).is_some() {
                    continue;
                }
                // Unknown names keep the exact full-byte contract through a bounded lookup.
                let Some(id) = KeyName::new(name) else {
                    continue;
                };
                let Ok(index) = self.keys.binary_search_by(|(stored, _)| stored.cmp(&id)) else {
                    continue;
                };
                let key = &self.keys[index].1;
                result.attack |= needed.attack && key.winning_attack & (1 << actual) != 0;
                result.jump |= needed.jump && key.winning_jump & (1 << actual) != 0;
            }
        }
        result
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

/// Dense identities for current desktop names make frame work independent of bindings,
/// modifier combinations and match order. Unknown names use the lossless bounded fallback.
#[inline(always)]
fn native_key(name: &str) -> Option<usize> {
    match name.as_bytes() {
        [b'K', b'e', b'y', key @ b'A'..=b'Z'] => return Some(usize::from(key - b'A')),
        [b'D', b'i', b'g', b'i', b't', digit @ b'0'..=b'9'] => {
            return Some(26 + usize::from(digit - b'0'));
        }
        [b'F', digit @ b'1'..=b'9'] => return Some(36 + usize::from(digit - b'1')),
        [b'F', tens @ b'1'..=b'2', ones @ b'0'..=b'9'] => {
            let number = usize::from(tens - b'0') * 10 + usize::from(ones - b'0');
            if number <= 24 {
                return Some(35 + number);
            }
        }
        _ => {}
    }
    Some(match name {
        "MouseLeft" => 60,
        "MouseRight" => 61,
        "MouseMiddle" => 62,
        "Unidentified" => 63,
        "ShiftLeft" => 64,
        "ShiftRight" => 65,
        "ControlLeft" => 66,
        "ControlRight" => 67,
        "AltLeft" => 68,
        "AltRight" => 69,
        "SuperLeft" => 70,
        "SuperRight" => 71,
        "Backspace" => 72,
        "Enter" => 73,
        "Escape" => 74,
        "Tab" => 75,
        "Space" => 76,
        "Minus" => 77,
        "Equal" => 78,
        "BracketLeft" => 79,
        "BracketRight" => 80,
        "Backslash" => 81,
        "Semicolon" => 82,
        "Quote" => 83,
        "Backquote" => 84,
        "Comma" => 85,
        "Period" => 86,
        "Slash" => 87,
        "Insert" => 88,
        "Home" => 89,
        "PageUp" => 90,
        "Delete" => 91,
        "End" => 92,
        "PageDown" => 93,
        "ArrowRight" => 94,
        "ArrowLeft" => 95,
        "ArrowDown" => 96,
        "ArrowUp" => 97,
        "CapsLock" => 98,
        "PrintScreen" => 99,
        "ScrollLock" => 100,
        "Pause" => 101,
        "NumLock" => 102,
        "NumpadDivide" => 103,
        "NumpadMultiply" => 104,
        "NumpadSubtract" => 105,
        "NumpadAdd" => 106,
        "NumpadEnter" => 107,
        "Numpad1" => 108,
        "Numpad2" => 109,
        "Numpad3" => 110,
        "Numpad4" => 111,
        "Numpad5" => 112,
        "Numpad6" => 113,
        "Numpad7" => 114,
        "Numpad8" => 115,
        "Numpad9" => 116,
        "Numpad0" => 117,
        "NumpadDecimal" => 118,
        "IntlBackslash" => 119,
        "ContextMenu" => 120,
        "NumpadEqual" => 121,
        "NumpadComma" => 122,
        _ => return None,
    })
}

/// IDs 64 through 71 are the four left/right modifier groups, in policy-mask order.
#[cfg(test)]
const NATIVE_NAMES: &[&str] = &[
    "KeyA",
    "KeyB",
    "KeyC",
    "KeyD",
    "KeyE",
    "KeyF",
    "KeyG",
    "KeyH",
    "KeyI",
    "KeyJ",
    "KeyK",
    "KeyL",
    "KeyM",
    "KeyN",
    "KeyO",
    "KeyP",
    "KeyQ",
    "KeyR",
    "KeyS",
    "KeyT",
    "KeyU",
    "KeyV",
    "KeyW",
    "KeyX",
    "KeyY",
    "KeyZ",
    "Digit0",
    "Digit1",
    "Digit2",
    "Digit3",
    "Digit4",
    "Digit5",
    "Digit6",
    "Digit7",
    "Digit8",
    "Digit9",
    "F1",
    "F2",
    "F3",
    "F4",
    "F5",
    "F6",
    "F7",
    "F8",
    "F9",
    "F10",
    "F11",
    "F12",
    "F13",
    "F14",
    "F15",
    "F16",
    "F17",
    "F18",
    "F19",
    "F20",
    "F21",
    "F22",
    "F23",
    "F24",
    "MouseLeft",
    "MouseRight",
    "MouseMiddle",
    "Unidentified",
    "ShiftLeft",
    "ShiftRight",
    "ControlLeft",
    "ControlRight",
    "AltLeft",
    "AltRight",
    "SuperLeft",
    "SuperRight",
    "Backspace",
    "Enter",
    "Escape",
    "Tab",
    "Space",
    "Minus",
    "Equal",
    "BracketLeft",
    "BracketRight",
    "Backslash",
    "Semicolon",
    "Quote",
    "Backquote",
    "Comma",
    "Period",
    "Slash",
    "Insert",
    "Home",
    "PageUp",
    "Delete",
    "End",
    "PageDown",
    "ArrowRight",
    "ArrowLeft",
    "ArrowDown",
    "ArrowUp",
    "CapsLock",
    "PrintScreen",
    "ScrollLock",
    "Pause",
    "NumLock",
    "NumpadDivide",
    "NumpadMultiply",
    "NumpadSubtract",
    "NumpadAdd",
    "NumpadEnter",
    "Numpad1",
    "Numpad2",
    "Numpad3",
    "Numpad4",
    "Numpad5",
    "Numpad6",
    "Numpad7",
    "Numpad8",
    "Numpad9",
    "Numpad0",
    "NumpadDecimal",
    "IntlBackslash",
    "ContextMenu",
    "NumpadEqual",
    "NumpadComma",
];

/// Action holds computed entirely inside this mod.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Holds {
    pub attack: bool,
    pub jump: bool,
}

#[cfg(test)]
mod tests;
