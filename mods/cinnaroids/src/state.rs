//! Personal module state and frame-driven click scheduling without operating-system input.

use serde::Serialize;
use serde_json::Value;

use crate::aim::Config;

pub const PRODUCT_NAME: &str = "Cinnaroids";
pub const PANEL_KEY: &str = "ShiftRight";
pub const STOP_KEY: &str = "F10";
const DEFAULT_CLICKER_KEY: &str = "F8";
const MIN_REACH: f32 = 3.0;
const MIN_CPS: u8 = 1;
const MAX_CPS: u8 = 30;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AimMode {
    Continuous,
    #[default]
    WhileClicking,
}

/// Enabled flags are deliberately not part of persisted preferences.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Preferences {
    pub cps: u8,
    pub clicker_key: String,
    pub aim_strength: u8,
    pub aim_mode: AimMode,
    pub reach_blocks: f32,
    pub dark_mode: bool,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            cps: 20,
            clicker_key: DEFAULT_CLICKER_KEY.into(),
            aim_strength: 35,
            aim_mode: AimMode::WhileClicking,
            reach_blocks: MIN_REACH,
            dark_mode: true,
        }
    }
}

impl Preferences {
    /// Invalid fields fall back independently, including older desktop-only fields.
    pub fn from_json(json: &str) -> Self {
        let mut preferences = Self::default();
        let Ok(Value::Object(values)) = serde_json::from_str(json) else {
            return preferences;
        };
        if let Some(value) = values.get("cps").and_then(Value::as_u64) {
            preferences.cps = value.clamp(u64::from(MIN_CPS), u64::from(MAX_CPS)) as u8;
        }
        if let Some(value) = values.get("aim_strength").and_then(Value::as_u64) {
            preferences.aim_strength = value.min(100) as u8;
        }
        if let Some(value) = values.get("aim_mode").and_then(Value::as_str) {
            preferences.aim_mode = match value {
                "continuous" => AimMode::Continuous,
                _ => AimMode::WhileClicking,
            };
        }
        if let Some(value) = values.get("reach_blocks").and_then(Value::as_f64)
            && value.is_finite()
        {
            preferences.reach_blocks =
                (value as f32).clamp(MIN_REACH, mod_api::MAX_ENTITY_REACH_BLOCKS);
        }
        if let Some(value) = values.get("dark_mode").and_then(Value::as_bool) {
            preferences.dark_mode = value;
        }
        let binding = values
            .get("clicker_key")
            .or_else(|| values.get("toggle_key"))
            .and_then(|value| {
                value
                    .as_str()
                    .map(str::to_owned)
                    .or_else(|| value.as_u64().and_then(legacy_key))
            });
        if let Some(binding) = binding.filter(|binding| valid_binding(binding)) {
            preferences.clicker_key = binding;
        }
        preferences
    }
}

#[derive(Clone, Debug)]
pub struct Event {
    pub id: String,
    pub value: f32,
}

pub struct Controls<'a> {
    pub focused: bool,
    pub panel_open: bool,
    pub keys: &'a [String],
    pub events: &'a [Event],
}

#[derive(Debug, Default)]
pub struct Modules {
    pub clicker: bool,
    pub aim: bool,
    pub reach: bool,
}

#[derive(Debug)]
pub struct State {
    pub preferences: Preferences,
    pub modules: Modules,
    pub capturing_key: bool,
    pub panel_dirty: bool,
    pub preferences_dirty: bool,
    pub reservations_dirty: bool,
    clicker: ClickCadence,
}

impl State {
    pub fn new(preferences: Preferences) -> Self {
        Self {
            preferences,
            modules: Modules::default(),
            capturing_key: false,
            panel_dirty: true,
            preferences_dirty: false,
            reservations_dirty: true,
            clicker: ClickCadence::default(),
        }
    }

    /// The stop edge has priority over panel events, key capture and toggles.
    pub fn controls(&mut self, controls: Controls<'_>) {
        if self.capturing_key && (!controls.focused || !controls.panel_open) {
            self.capturing_key = false;
            self.panel_dirty = true;
        }
        if !controls.focused {
            return;
        }
        if controls.keys.iter().any(|key| key == STOP_KEY)
            || (controls.panel_open && controls.events.iter().any(|event| event.id == "stop_all"))
        {
            self.stop_all();
            return;
        }
        if controls.panel_open {
            for event in controls.events {
                self.event(event);
            }
        }
        if self.capturing_key {
            if let Some(key) = controls.keys.iter().find(|key| valid_binding(key)) {
                self.preferences.clicker_key = key.clone();
                self.capturing_key = false;
                self.panel_dirty = true;
                self.preferences_dirty = true;
                self.reservations_dirty = true;
                self.clicker.reset();
            }
            return;
        }
        if controls
            .keys
            .iter()
            .any(|key| *key == self.preferences.clicker_key)
        {
            self.modules.clicker = !self.modules.clicker;
            self.clicker.reset();
            self.panel_dirty = true;
        }
    }

    fn event(&mut self, event: &Event) {
        let value = event.value;
        if !value.is_finite() {
            return;
        }
        let changed = match event.id.as_str() {
            "clicker" => replace(&mut self.modules.clicker, value >= 0.5),
            "cps" => replace(
                &mut self.preferences.cps,
                value.round().clamp(f32::from(MIN_CPS), f32::from(MAX_CPS)) as u8,
            ),
            "clicker_key" => {
                self.capturing_key = true;
                self.panel_dirty = true;
                return;
            }
            "aim" => replace(&mut self.modules.aim, value >= 0.5),
            "aim_strength" => replace(
                &mut self.preferences.aim_strength,
                value.round().clamp(0.0, 100.0) as u8,
            ),
            "aim_mode" => replace(
                &mut self.preferences.aim_mode,
                if value >= 0.5 {
                    AimMode::WhileClicking
                } else {
                    AimMode::Continuous
                },
            ),
            "reach" => replace(&mut self.modules.reach, value >= 0.5),
            "reach_blocks" => replace(
                &mut self.preferences.reach_blocks,
                ((value * 10.0).round() / 10.0).clamp(MIN_REACH, mod_api::MAX_ENTITY_REACH_BLOCKS),
            ),
            "dark_mode" => replace(&mut self.preferences.dark_mode, value >= 0.5),
            "stop_all" => {
                self.stop_all();
                return;
            }
            _ => false,
        };
        if changed {
            self.panel_dirty = true;
            if matches!(event.id.as_str(), "clicker" | "cps") {
                self.clicker.reset();
            }
            if !matches!(event.id.as_str(), "clicker" | "aim" | "reach") {
                self.preferences_dirty = true;
            }
        }
    }

    pub fn stop_all(&mut self) {
        self.modules = Modules::default();
        self.capturing_key = false;
        self.clicker.reset();
        self.panel_dirty = true;
    }

    pub fn aim_config(&self) -> Config {
        Config {
            enabled: self.modules.aim,
            strength: self.preferences.aim_strength,
            only_when_clicking: self.preferences.aim_mode == AimMode::WhileClicking,
        }
    }

    pub fn attack_reach(&self, gameplay: bool) -> Option<f32> {
        (self.modules.reach && gameplay).then_some(self.preferences.reach_blocks)
    }

    pub fn attack_pulse(&mut self, frame: Option<(u64, i32, bool, f32)>) -> bool {
        self.clicker
            .update(self.modules.clicker, self.preferences.cps, frame)
    }

    pub fn reserved_keys(&self) -> Vec<String> {
        vec![
            PANEL_KEY.into(),
            STOP_KEY.into(),
            self.preferences.clicker_key.clone(),
        ]
    }

    pub fn panel_json(&self) -> Result<String, serde_json::Error> {
        let key_label = format!(
            "Toggle key: {}",
            if self.capturing_key {
                "Press a key"
            } else {
                &self.preferences.clicker_key
            }
        );
        let controls = vec![
            Control::Toggle {
                id: "clicker",
                label: "Clicker",
                value: self.modules.clicker,
            },
            Control::Slider {
                id: "cps",
                label: "CPS",
                value: f32::from(self.preferences.cps),
                min: f32::from(MIN_CPS),
                max: f32::from(MAX_CPS),
                step: 1.0,
            },
            Control::Button {
                id: "clicker_key",
                label: &key_label,
            },
            Control::Toggle {
                id: "aim",
                label: "Aim assist",
                value: self.modules.aim,
            },
            Control::Slider {
                id: "aim_strength",
                label: "Strength",
                value: f32::from(self.preferences.aim_strength),
                min: 0.0,
                max: 100.0,
                step: 1.0,
            },
            Control::Choice {
                id: "aim_mode",
                label: "Activation",
                index: u32::from(self.preferences.aim_mode == AimMode::WhileClicking),
                options: ["Continuous", "While clicking"],
            },
            Control::Toggle {
                id: "reach",
                label: "Reach",
                value: self.modules.reach,
            },
            Control::Slider {
                id: "reach_blocks",
                label: "Distance",
                value: self.preferences.reach_blocks,
                min: MIN_REACH,
                max: mod_api::MAX_ENTITY_REACH_BLOCKS,
                step: 0.1,
            },
            Control::Toggle {
                id: "dark_mode",
                label: "Dark mode",
                value: self.preferences.dark_mode,
            },
            Control::Button {
                id: "stop_all",
                label: "Disable all",
            },
        ];
        serde_json::to_string(&Panel {
            title: PRODUCT_NAME,
            toggle_key: PANEL_KEY,
            dark: self.preferences.dark_mode,
            capture_key: self.capturing_key,
            controls,
            sections: vec![
                Section {
                    id: "clicker_section",
                    label: "Clicker",
                    category: "Combat",
                    toggle: Some("clicker"),
                    controls: &["cps", "clicker_key"],
                },
                Section {
                    id: "aim_section",
                    label: "Aim assist",
                    category: "Combat",
                    toggle: Some("aim"),
                    controls: &["aim_strength", "aim_mode"],
                },
                Section {
                    id: "reach_section",
                    label: "Reach",
                    category: "Combat",
                    toggle: Some("reach"),
                    controls: &["reach_blocks"],
                },
                Section {
                    id: "general_section",
                    label: "General",
                    category: "Settings",
                    toggle: None,
                    controls: &["dark_mode", "stop_all"],
                },
            ],
        })
    }
}

fn replace<T: PartialEq>(target: &mut T, value: T) -> bool {
    if *target == value {
        false
    } else {
        *target = value;
        true
    }
}

/// A held attack begins with the physical press, followed by bounded repeat edges.
#[derive(Debug, Default)]
struct ClickCadence {
    context: Option<(u64, i32)>,
    held: bool,
    elapsed: f32,
}

impl ClickCadence {
    fn reset(&mut self) {
        *self = Self::default();
    }

    fn update(&mut self, enabled: bool, cps: u8, frame: Option<(u64, i32, bool, f32)>) -> bool {
        let Some((session, dimension, attack_held, seconds)) = frame else {
            self.reset();
            return false;
        };
        if !enabled || !attack_held || !seconds.is_finite() || seconds < 0.0 {
            self.reset();
            return false;
        }
        let context = (session, dimension);
        if self.context != Some(context) || !self.held {
            self.context = Some(context);
            self.held = true;
            self.elapsed = 0.0;
            return false;
        }
        let interval = 1.0 / f32::from(cps.clamp(MIN_CPS, MAX_CPS));
        self.elapsed += seconds;
        if self.elapsed < interval {
            return false;
        }
        // Retain fractional normal-frame time, but never catch up missed clicks.
        self.elapsed = if seconds >= interval {
            0.0
        } else {
            self.elapsed - interval
        };
        true
    }
}

fn valid_binding(key: &str) -> bool {
    !key.is_empty()
        && key.len() <= mod_api::MAX_CONTROL_KEY_BYTES
        && key.bytes().all(|byte| byte.is_ascii_alphanumeric())
        && key != PANEL_KEY
        && key != STOP_KEY
}

fn legacy_key(key: u64) -> Option<String> {
    Some(match key {
        0x30..=0x39 => format!("Digit{}", char::from_u32(key as u32)?),
        0x41..=0x5A => format!("Key{}", char::from_u32(key as u32)?),
        0x70..=0x87 => format!("F{}", key - 0x6F),
        0x08 => "Backspace".into(),
        0x09 => "Tab".into(),
        0x0D => "Enter".into(),
        0x10 | 0xA0 => "ShiftLeft".into(),
        0x11 | 0xA2 => "ControlLeft".into(),
        0x12 | 0xA4 => "AltLeft".into(),
        0x14 => "CapsLock".into(),
        0x1B => "Escape".into(),
        0x20 => "Space".into(),
        0x21 => "PageUp".into(),
        0x22 => "PageDown".into(),
        0x23 => "End".into(),
        0x24 => "Home".into(),
        0x25 => "ArrowLeft".into(),
        0x26 => "ArrowUp".into(),
        0x27 => "ArrowRight".into(),
        0x28 => "ArrowDown".into(),
        0x2D => "Insert".into(),
        0x2E => "Delete".into(),
        0x60..=0x69 => format!("Numpad{}", key - 0x60),
        0xA1 => PANEL_KEY.into(),
        0xA3 => "ControlRight".into(),
        0xA5 => "AltRight".into(),
        _ => return None,
    })
}

#[derive(Serialize)]
struct Panel<'a> {
    title: &'a str,
    toggle_key: &'a str,
    dark: bool,
    capture_key: bool,
    controls: Vec<Control<'a>>,
    sections: Vec<Section<'a>>,
}

#[derive(Serialize)]
struct Section<'a> {
    id: &'a str,
    label: &'a str,
    category: &'a str,
    toggle: Option<&'a str>,
    controls: &'a [&'a str],
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Control<'a> {
    Toggle {
        id: &'a str,
        label: &'a str,
        value: bool,
    },
    Slider {
        id: &'a str,
        label: &'a str,
        value: f32,
        min: f32,
        max: f32,
        step: f32,
    },
    Button {
        id: &'a str,
        label: &'a str,
    },
    Choice {
        id: &'a str,
        label: &'a str,
        index: u32,
        options: [&'a str; 2],
    },
}

#[cfg(test)]
mod tests;
