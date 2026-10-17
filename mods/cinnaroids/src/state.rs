//! Personal module state and frame-driven click scheduling without operating-system input.

use serde::Serialize;
use serde_json::Value;

use crate::aim::Config;
use crate::jump_reset::{DEFAULT_WINDOW_TICKS, MAX_WINDOW_TICKS, MIN_WINDOW_TICKS};

pub const PRODUCT_NAME: &str = "Cinnaroids";
pub const PANEL_KEY: &str = "F7";
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
    pub menu_key: String,
    pub clicker_key: String,
    pub aim_key: String,
    pub reach_key: String,
    pub jump_reset_key: String,
    pub jump_reset_window_ticks: u8,
    pub aim_strength: u8,
    pub aim_mode: AimMode,
    pub reach_blocks: f32,
    pub fake_lag_ms: u16,
    pub show_real_position: bool,
    pub netherite_range: u8,
    /// Column origins relative to the reference box centre: Combat, Network, Render, Menu.
    pub columns: [[f32; 2]; 4],
    pub collapsed: [bool; 4],
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            cps: 20,
            menu_key: PANEL_KEY.into(),
            clicker_key: String::new(),
            aim_key: String::new(),
            reach_key: String::new(),
            jump_reset_key: String::new(),
            jump_reset_window_ticks: DEFAULT_WINDOW_TICKS,
            aim_strength: 35,
            aim_mode: AimMode::WhileClicking,
            reach_blocks: MIN_REACH,
            fake_lag_ms: 100,
            show_real_position: false,
            netherite_range: 64,
            columns: crate::panel::DEFAULT_COLUMNS,
            collapsed: [false; 4],
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
        if let Some(Value::Array(columns)) = values.get("columns") {
            for (column, (target, value)) in preferences.columns.iter_mut().zip(columns).enumerate()
            {
                let position = value.as_array().and_then(|pair| match pair.as_slice() {
                    [x, y] => Some([x.as_f64()? as f32, y.as_f64()? as f32]),
                    _ => None,
                });
                if let Some(position) = position.filter(|p| p.iter().all(|v| v.is_finite())) {
                    *target = clamp_column(column, position);
                }
            }
        }
        if let Some(Value::Array(collapsed)) = values.get("collapsed") {
            for (target, value) in preferences.collapsed.iter_mut().zip(collapsed) {
                if let Some(value) = value.as_bool() {
                    *target = value;
                }
            }
        }
        if let Some(value) = values.get("fake_lag_ms").and_then(Value::as_u64) {
            preferences.fake_lag_ms = value.min(u64::from(mod_api::MAX_PACKET_DELAY_MS)) as u16;
        }
        if let Some(value) = values.get("show_real_position").and_then(Value::as_bool) {
            preferences.show_real_position = value;
        }
        if let Some(value) = values.get("netherite_range").and_then(Value::as_u64) {
            preferences.netherite_range =
                value.clamp(1, mod_api::MAX_BLOCK_HIGHLIGHT_RANGE as u64) as u8;
        }
        if let Some(value) = values
            .get("jump_reset_window_ticks")
            .and_then(Value::as_u64)
        {
            preferences.jump_reset_window_ticks =
                value.clamp(u64::from(MIN_WINDOW_TICKS), u64::from(MAX_WINDOW_TICKS)) as u8;
        }
        if let Some(key) = values.get("menu_key").and_then(Value::as_str)
            && valid_binding(key)
        {
            preferences.menu_key = key.into();
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
        if let Some(binding) =
            binding.filter(|binding| binding.is_empty() || valid_binding(binding))
        {
            preferences.clicker_key = binding;
        }
        for (field, target) in [
            ("aim_key", &mut preferences.aim_key),
            ("reach_key", &mut preferences.reach_key),
            ("jump_reset_key", &mut preferences.jump_reset_key),
        ] {
            if let Some(binding) = values.get(field).and_then(Value::as_str)
                && (binding.is_empty() || valid_binding(binding))
            {
                *target = binding.into();
            }
        }
        // Keep the first saved assignment; conflicting module bindings become unbound.
        if !preferences.aim_key.is_empty() && preferences.aim_key == preferences.clicker_key {
            preferences.aim_key.clear();
        }
        if !preferences.reach_key.is_empty()
            && (preferences.reach_key == preferences.clicker_key
                || preferences.reach_key == preferences.aim_key)
        {
            preferences.reach_key.clear();
        }
        if !preferences.jump_reset_key.is_empty()
            && [
                &preferences.clicker_key,
                &preferences.aim_key,
                &preferences.reach_key,
            ]
            .contains(&&preferences.jump_reset_key)
        {
            preferences.jump_reset_key.clear();
        }
        if [
            &preferences.clicker_key,
            &preferences.aim_key,
            &preferences.reach_key,
            &preferences.jump_reset_key,
        ]
        .contains(&&preferences.menu_key)
        {
            preferences.menu_key = [PANEL_KEY, "F6", "F5", "F4", "F3"]
                .into_iter()
                .find(|key| {
                    ![
                        preferences.clicker_key.as_str(),
                        preferences.aim_key.as_str(),
                        preferences.reach_key.as_str(),
                        preferences.jump_reset_key.as_str(),
                    ]
                    .contains(key)
                })
                .expect("more menu fallbacks than module bindings")
                .into();
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
    /// Physical keys held this frame; a held MouseLeft keeps a column drag alive.
    pub held: &'a [String],
    pub seconds: f32,
    pub events: &'a [Event],
}

/// One column drag: the pointer's offset from the column origin and whether it moved.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Drag {
    pub column: usize,
    grab: [f32; 2],
    start: [f32; 2],
    moved: bool,
}

/// Seconds the menu column stays hit-testable while its exit fade plays.
const MENU_FADE_SECONDS: f32 = 0.2;
/// Pointer travel, in logical units, that turns a header press into a drag.
const DRAG_THRESHOLD: f32 = 3.0;
/// Published pad value while no drag is active; a real pointer never maps here.
const PAD_IDLE: f32 = -crate::panel::PAD_HALF;

/// Bound a column origin. Module columns may go anywhere the shared pointer pad reaches;
/// a drag keeps the grabbed header under the on-screen pointer. The Menu column, which holds
/// Reset layout, stays inside the reference box the host always fits on screen.
pub fn clamp_column(column: usize, [x, y]: [f32; 2]) -> [f32; 2] {
    let [width, height] = if column == 3 {
        crate::panel::REFERENCE
    } else {
        [2.0 * crate::panel::PAD_HALF; 2]
    };
    [
        x.clamp(-width / 2.0, width / 2.0 - crate::panel::COLUMN_WIDTH),
        y.clamp(-height / 2.0, height / 2.0 - 22.0),
    ]
}

#[derive(Debug, Default)]
pub struct Modules {
    pub clicker: bool,
    pub aim: bool,
    pub reach: bool,
    pub fake_lag: bool,
    pub netherite: bool,
    pub fullbright: bool,
    pub jump_reset: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeybindTarget {
    Menu,
    Clicker,
    Aim,
    Reach,
    JumpReset,
}

#[derive(Debug)]
pub struct State {
    pub preferences: Preferences,
    pub modules: Modules,
    pub capturing_key: Option<KeybindTarget>,
    pub panel_dirty: bool,
    /// Transient disclosure state, independent from enabled flags and persisted settings.
    pub expanded: [bool; 6],
    pub menu_open: bool,
    menu_fade: f32,
    pub drag: Option<Drag>,
    press_armed: bool,
    pointer: [f32; 2],
    pub preferences_dirty: bool,
    pub reservations_dirty: bool,
    pub finder_dirty: bool,
    pub fullbright_dirty: bool,
    clicker: ClickCadence,
}

impl State {
    pub fn new(preferences: Preferences) -> Self {
        Self {
            preferences,
            modules: Modules::default(),
            capturing_key: None,
            panel_dirty: true,
            expanded: [false; 6],
            menu_open: false,
            menu_fade: 0.0,
            drag: None,
            press_armed: false,
            pointer: [PAD_IDLE; 2],
            preferences_dirty: false,
            reservations_dirty: true,
            finder_dirty: true,
            fullbright_dirty: true,
            clicker: ClickCadence::default(),
        }
    }

    /// Apply focused panel events and explicitly assigned shortcuts without leaking capture edges.
    pub fn controls(&mut self, controls: Controls<'_>) {
        let cancelled_capture =
            self.capturing_key.is_some() && (!controls.focused || !controls.panel_open);
        if cancelled_capture {
            self.capturing_key = None;
            self.panel_dirty = true;
        }
        if self.drag.is_some() && (!controls.focused || !controls.panel_open) {
            self.end_drag(false);
        }
        if self.menu_fade > 0.0 {
            self.menu_fade -= controls.seconds.max(0.0);
            if self.menu_fade <= 0.0 {
                self.menu_fade = 0.0;
                self.panel_dirty = true;
            }
        }
        if !controls.focused {
            return;
        }
        if cancelled_capture {
            return;
        }
        if controls.panel_open {
            if controls.keys.iter().any(|key| key == "MouseLeft") {
                if self.drag.is_some() {
                    self.end_drag(false);
                }
                self.press_armed = true;
            }
            let mut touched = false;
            for event in controls.events {
                let axis = match event.id.as_str() {
                    "drag_x" => 0,
                    "drag_y" => 1,
                    _ => {
                        self.event(event);
                        continue;
                    }
                };
                if event.value.is_finite() {
                    self.pointer[axis] = event.value;
                    touched = true;
                }
            }
            if touched {
                self.drag_pointer();
            }
            if self.drag.is_some() && !controls.held.iter().any(|key| key == "MouseLeft") {
                self.end_drag(true);
            }
        }
        if let Some(target) = self.capturing_key {
            if controls.keys.iter().any(|key| key == "Escape") {
                if target != KeybindTarget::Menu {
                    self.binding_mut(target).clear();
                    self.preferences_dirty = true;
                    self.reservations_dirty = true;
                    if target == KeybindTarget::Clicker {
                        self.clicker.reset();
                    }
                }
                self.capturing_key = None;
                self.panel_dirty = true;
                return;
            }
            if let Some(key) = controls.keys.iter().find(|key| valid_binding(key))
                && !self.binding_conflicts(target, key)
            {
                *self.binding_mut(target) = key.clone();
                self.capturing_key = None;
                self.panel_dirty = true;
                self.preferences_dirty = true;
                self.reservations_dirty = true;
                if target == KeybindTarget::Clicker {
                    self.clicker.reset();
                }
            }
            return;
        }
        for key in controls.keys.iter().filter(|key| !key.is_empty()) {
            if *key == self.preferences.clicker_key {
                self.modules.clicker = !self.modules.clicker;
                self.clicker.reset();
                self.panel_dirty = true;
            } else if *key == self.preferences.aim_key {
                self.modules.aim = !self.modules.aim;
                self.panel_dirty = true;
            } else if *key == self.preferences.jump_reset_key {
                self.modules.jump_reset = !self.modules.jump_reset;
                self.panel_dirty = true;
            } else if *key == self.preferences.reach_key {
                self.modules.reach = !self.modules.reach;
                self.panel_dirty = true;
            }
        }
    }

    fn binding_mut(&mut self, target: KeybindTarget) -> &mut String {
        match target {
            KeybindTarget::Menu => &mut self.preferences.menu_key,
            KeybindTarget::Clicker => &mut self.preferences.clicker_key,
            KeybindTarget::Aim => &mut self.preferences.aim_key,
            KeybindTarget::Reach => &mut self.preferences.reach_key,
            KeybindTarget::JumpReset => &mut self.preferences.jump_reset_key,
        }
    }

    fn binding_conflicts(&self, target: KeybindTarget, key: &str) -> bool {
        [
            (KeybindTarget::Menu, &self.preferences.menu_key),
            (KeybindTarget::Clicker, &self.preferences.clicker_key),
            (KeybindTarget::Aim, &self.preferences.aim_key),
            (KeybindTarget::Reach, &self.preferences.reach_key),
            (KeybindTarget::JumpReset, &self.preferences.jump_reset_key),
        ]
        .iter()
        .any(|(other, binding)| *other != target && key == *binding)
    }

    /// Start, continue or ignore a column drag from this frame's pointer-pad samples.
    /// Every header shares one screen-fixed pad that publishes an unreachable idle value,
    /// so each press reports both axes; the press point selects the topmost header.
    fn drag_pointer(&mut self) {
        let pointer = self.pointer;
        if pointer.iter().any(|value| *value <= PAD_IDLE) {
            return;
        }
        match self.drag {
            Some(mut drag) => {
                let travel = (pointer[0] - drag.start[0]).hypot(pointer[1] - drag.start[1]);
                drag.moved |= travel >= DRAG_THRESHOLD;
                if drag.moved {
                    let origin = [pointer[0] - drag.grab[0], pointer[1] - drag.grab[1]];
                    self.preferences.columns[drag.column] = clamp_column(drag.column, origin);
                }
                self.drag = Some(drag);
                self.panel_dirty = true;
            }
            None if self.press_armed => {
                self.press_armed = false;
                let Some(column) = self.header_at(pointer) else {
                    return;
                };
                let [x, y] = self.preferences.columns[column];
                self.drag = Some(Drag {
                    column,
                    grab: [pointer[0] - x, pointer[1] - y],
                    start: pointer,
                    moved: false,
                });
                self.capturing_key = None;
                self.panel_dirty = true;
            }
            None => {}
        }
    }

    /// The topmost visible column header under a pointer, in draw order.
    fn header_at(&self, [x, y]: [f32; 2]) -> Option<usize> {
        (0..4).rev().find(|column| {
            let [left, top] = self.preferences.columns[*column];
            (*column < 3 || self.menu_visible())
                && (left..=left + crate::panel::COLUMN_WIDTH).contains(&x)
                && (top..=top + 22.0).contains(&y)
        })
    }

    /// Finish a drag; a header press that never moved collapses or expands its column.
    fn end_drag(&mut self, released: bool) {
        let Some(drag) = self.drag.take() else {
            return;
        };
        if released && !drag.moved {
            self.preferences.collapsed[drag.column] = !self.preferences.collapsed[drag.column];
        }
        if released {
            self.preferences_dirty = true;
        }
        self.pointer = [PAD_IDLE; 2];
        self.press_armed = false;
        self.panel_dirty = true;
    }

    /// Runtime enabled flags in panel order.
    pub fn enabled_flags(&self) -> [bool; 7] {
        let m = &self.modules;
        [
            m.clicker,
            m.aim,
            m.reach,
            m.jump_reset,
            m.fake_lag,
            m.netherite,
            m.fullbright,
        ]
    }

    pub fn key_for(&self, target: KeybindTarget) -> &str {
        match target {
            KeybindTarget::Menu => &self.preferences.menu_key,
            KeybindTarget::Clicker => &self.preferences.clicker_key,
            KeybindTarget::Aim => &self.preferences.aim_key,
            KeybindTarget::Reach => &self.preferences.reach_key,
            KeybindTarget::JumpReset => &self.preferences.jump_reset_key,
        }
    }

    /// The menu column stays hit-testable only while open or fading out.
    pub fn menu_visible(&self) -> bool {
        self.menu_open || self.menu_fade > 0.0
    }

    fn event(&mut self, event: &Event) {
        let value = event.value;
        if !value.is_finite() {
            return;
        }
        let changed = match event.id.as_str() {
            "expand_clicker" | "expand_aim" | "expand_reach" | "expand_jump_reset"
            | "expand_fake_lag" | "expand_netherite" => {
                let index = [
                    "expand_clicker",
                    "expand_aim",
                    "expand_reach",
                    "expand_jump_reset",
                    "expand_fake_lag",
                    "expand_netherite",
                ]
                .iter()
                .position(|id| *id == event.id)
                .unwrap();
                self.expanded[index] = !self.expanded[index];
                self.capturing_key = None;
                self.panel_dirty = true;
                return;
            }
            "menu_settings" => {
                self.menu_open = !self.menu_open;
                self.menu_fade = if self.menu_open {
                    0.0
                } else {
                    MENU_FADE_SECONDS
                };
                self.capturing_key = None;
                self.panel_dirty = true;
                return;
            }
            "reset_layout" => {
                let defaults = Preferences::default();
                self.preferences.columns = defaults.columns;
                self.preferences.collapsed = defaults.collapsed;
                self.preferences_dirty = true;
                self.panel_dirty = true;
                return;
            }
            "clicker" => replace(&mut self.modules.clicker, value >= 0.5),
            "cps" => replace(
                &mut self.preferences.cps,
                value.round().clamp(f32::from(MIN_CPS), f32::from(MAX_CPS)) as u8,
            ),
            "menu_key" | "clicker_key" | "aim_key" | "reach_key" | "jump_reset_key" => {
                self.capturing_key = Some(match event.id.as_str() {
                    "menu_key" => KeybindTarget::Menu,
                    "aim_key" => KeybindTarget::Aim,
                    "reach_key" => KeybindTarget::Reach,
                    "jump_reset_key" => KeybindTarget::JumpReset,
                    _ => KeybindTarget::Clicker,
                });
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
            "jump_reset" => replace(&mut self.modules.jump_reset, value >= 0.5),
            "jump_reset_window_ticks" => replace(
                &mut self.preferences.jump_reset_window_ticks,
                value
                    .round()
                    .clamp(f32::from(MIN_WINDOW_TICKS), f32::from(MAX_WINDOW_TICKS))
                    as u8,
            ),
            "fake_lag" => replace(&mut self.modules.fake_lag, value >= 0.5),
            "fake_lag_ms" => replace(
                &mut self.preferences.fake_lag_ms,
                value
                    .round()
                    .clamp(0.0, mod_api::MAX_PACKET_DELAY_MS as f32) as u16,
            ),
            "show_real_position" => replace(&mut self.preferences.show_real_position, value >= 0.5),
            "netherite" => replace(&mut self.modules.netherite, value >= 0.5),
            "fullbright" => replace(&mut self.modules.fullbright, value >= 0.5),
            "netherite_range" => replace(
                &mut self.preferences.netherite_range,
                value.round().clamp(1.0, mod_api::MAX_BLOCK_HIGHLIGHT_RANGE) as u8,
            ),
            _ => false,
        };
        if changed {
            self.panel_dirty = true;
            if event.id == "fullbright" {
                self.fullbright_dirty = true;
            }
            if matches!(event.id.as_str(), "netherite" | "netherite_range") {
                self.finder_dirty = true;
            }
            if matches!(event.id.as_str(), "clicker" | "cps") {
                self.clicker.reset();
            }
            if !matches!(
                event.id.as_str(),
                "clicker"
                    | "aim"
                    | "reach"
                    | "fake_lag"
                    | "netherite"
                    | "fullbright"
                    | "jump_reset"
            ) {
                self.preferences_dirty = true;
            }
        }
    }

    pub fn aim_config(&self) -> Config {
        Config {
            enabled: self.modules.aim,
            strength: self.preferences.aim_strength,
            only_when_clicking: self.preferences.aim_mode == AimMode::WhileClicking,
        }
    }

    pub fn packet_delay_ms(&self) -> u32 {
        if self.modules.fake_lag {
            u32::from(self.preferences.fake_lag_ms)
        } else {
            0
        }
    }

    pub fn show_real_position(&self) -> bool {
        self.packet_delay_ms() != 0 && self.preferences.show_real_position
    }

    pub fn block_highlights(&self) -> Option<mod_api::BlockHighlightSpec> {
        self.modules.netherite.then(|| mod_api::BlockHighlightSpec {
            identifiers: vec![
                "minecraft:ancient_debris".into(),
                "minecraft:netherite_block".into(),
            ],
            range: f32::from(self.preferences.netherite_range),
            color: [1.0, 0.12, 0.55, 1.0],
        })
    }

    pub fn attack_reach(&self, gameplay: bool) -> Option<f32> {
        (self.modules.reach && gameplay).then_some(self.preferences.reach_blocks)
    }

    pub fn attack_pulse(&mut self, frame: Option<(u64, i32, bool, f32)>) -> bool {
        self.clicker
            .update(self.modules.clicker, self.preferences.cps, frame)
    }

    pub fn reserved_keys(&self) -> Vec<String> {
        let mut keys = vec![self.preferences.menu_key.clone()];
        for key in [
            &self.preferences.clicker_key,
            &self.preferences.aim_key,
            &self.preferences.reach_key,
            &self.preferences.jump_reset_key,
        ] {
            if !key.is_empty() && !keys.contains(key) {
                keys.push(key.clone());
            }
        }
        keys
    }

    /// Full standalone specification for packaging and native previews.
    pub fn panel_json(&self) -> Result<String, serde_json::Error> {
        self.panel_content(true)
    }

    /// Publish current control values against a tiny placeholder before selecting the retained route.
    /// Built from static fragments: serde's escaping and float formatting would not fit the
    /// host's per-frame fuel alongside the binding patch.
    pub fn panel_shell_json(&self) -> Result<String, serde_json::Error> {
        let mut json = String::with_capacity(3072);
        crate::panel::shell(&mut json, self);
        Ok(json)
    }

    /// The published value of a drag pad axis: the pointer while dragging, otherwise idle.
    pub fn pad_value(&self, axis: usize) -> f32 {
        if self.drag.is_some() {
            self.pointer[axis]
        } else {
            PAD_IDLE
        }
    }

    /// Serde reference for the hand-built shell; tests keep both schemas identical.
    #[cfg(test)]
    pub fn panel_reference_json(&self) -> Result<String, serde_json::Error> {
        self.panel_content(false)
    }

    /// Live route and scalar patch for the component's embedded panel catalog.
    pub fn panel_update_json(&self) -> Result<String, serde_json::Error> {
        crate::panel::route_update(self)
    }

    /// Serialize the common schema without resolving or cloning native geometry.
    fn panel_content(&self, full: bool) -> Result<String, serde_json::Error> {
        let mut controls = vec![
            Control::Keybind {
                id: "menu_key",
                label: "Menu key",
                key: crate::panel::key_legend(&self.preferences.menu_key),
                capturing: self.capturing_key == Some(KeybindTarget::Menu),
            },
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
            Control::Keybind {
                id: "clicker_key",
                label: "Clicker key",
                key: crate::panel::key_legend(&self.preferences.clicker_key),
                capturing: self.capturing_key == Some(KeybindTarget::Clicker),
            },
            Control::Toggle {
                id: "aim",
                label: "Aim assist",
                value: self.modules.aim,
            },
            Control::Slider {
                id: "aim_strength",
                label: "Aim strength",
                value: f32::from(self.preferences.aim_strength),
                min: 0.0,
                max: 100.0,
                step: 1.0,
            },
            Control::Choice {
                id: "aim_mode",
                label: "Mode",
                index: u32::from(self.preferences.aim_mode == AimMode::WhileClicking),
                options: ["Continuous", "While clicking"],
            },
            Control::Keybind {
                id: "aim_key",
                label: "Aim assist key",
                key: crate::panel::key_legend(&self.preferences.aim_key),
                capturing: self.capturing_key == Some(KeybindTarget::Aim),
            },
            Control::Toggle {
                id: "reach",
                label: "Reach",
                value: self.modules.reach,
            },
            Control::Slider {
                id: "reach_blocks",
                label: "Reach distance",
                value: self.preferences.reach_blocks,
                min: MIN_REACH,
                max: mod_api::MAX_ENTITY_REACH_BLOCKS,
                step: 0.1,
            },
            Control::Keybind {
                id: "reach_key",
                label: "Reach key",
                key: crate::panel::key_legend(&self.preferences.reach_key),
                capturing: self.capturing_key == Some(KeybindTarget::Reach),
            },
            Control::Toggle {
                id: "jump_reset",
                label: "Auto Jump Reset",
                value: self.modules.jump_reset,
            },
            Control::Slider {
                id: "jump_reset_window_ticks",
                label: "Hit window (ticks)",
                value: f32::from(self.preferences.jump_reset_window_ticks),
                min: f32::from(MIN_WINDOW_TICKS),
                max: f32::from(MAX_WINDOW_TICKS),
                step: 1.0,
            },
            Control::Keybind {
                id: "jump_reset_key",
                label: "Jump reset key",
                key: crate::panel::key_legend(&self.preferences.jump_reset_key),
                capturing: self.capturing_key == Some(KeybindTarget::JumpReset),
            },
            Control::Toggle {
                id: "fake_lag",
                label: "FakeLag",
                value: self.modules.fake_lag,
            },
            Control::Slider {
                id: "fake_lag_ms",
                label: "FakeLag delay (ms)",
                value: f32::from(self.preferences.fake_lag_ms),
                min: 0.0,
                max: mod_api::MAX_PACKET_DELAY_MS as f32,
                step: 1.0,
            },
            Control::Toggle {
                id: "show_real_position",
                label: "Show real position",
                value: self.preferences.show_real_position,
            },
            Control::Toggle {
                id: "netherite",
                label: "Netherite Finder",
                value: self.modules.netherite,
            },
            Control::Slider {
                id: "netherite_range",
                label: "Finder range",
                value: f32::from(self.preferences.netherite_range),
                min: 1.0,
                max: mod_api::MAX_BLOCK_HIGHLIGHT_RANGE,
                step: 1.0,
            },
            Control::Toggle {
                id: "fullbright",
                label: "Fullbright",
                value: self.modules.fullbright,
            },
            Control::Button {
                id: "expand_clicker",
                label: "Clicker settings",
            },
            Control::Button {
                id: "expand_aim",
                label: "Aim assist settings",
            },
            Control::Button {
                id: "expand_reach",
                label: "Reach settings",
            },
            Control::Button {
                id: "expand_jump_reset",
                label: "Auto Jump Reset settings",
            },
            Control::Button {
                id: "expand_fake_lag",
                label: "FakeLag settings",
            },
            Control::Button {
                id: "expand_netherite",
                label: "Netherite Finder settings",
            },
            Control::Button {
                id: "menu_settings",
                label: "Menu settings",
            },
            Control::Button {
                id: "reset_layout",
                label: "Reset layout",
            },
        ];
        for (axis, (id, label)) in [("drag_x", "Column drag x"), ("drag_y", "Column drag y")]
            .into_iter()
            .enumerate()
        {
            controls.push(Control::Slider {
                id,
                label,
                value: self.pad_value(axis),
                min: -crate::panel::PAD_HALF,
                max: crate::panel::PAD_HALF,
                step: 0.5,
            });
        }
        let panel = Panel {
            title: PRODUCT_NAME,
            toggle_key: &self.preferences.menu_key,
            dark: true,
            capture_key: self.capturing_key.is_some(),
            style: "compact",
            reference_size: crate::panel::REFERENCE,
            motion: true,
            controls,
        };
        let mut json = serde_json::to_string(&panel)?;
        if full {
            crate::panel::append(&mut json, self)?;
        } else {
            crate::panel::append_shell(&mut json);
        }
        Ok(json)
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

/// Keep primary clicks and Escape available for panel controls and keybind clearing.
fn valid_binding(key: &str) -> bool {
    !key.is_empty()
        && key.len() <= mod_api::MAX_CONTROL_KEY_BYTES
        && key.bytes().all(|byte| byte.is_ascii_alphanumeric())
        && key != "MouseLeft"
        && key != "Escape"
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
        0xA1 => "ShiftRight".into(),
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
    style: &'a str,
    reference_size: [f32; 2],
    motion: bool,
    controls: Vec<Control<'a>>,
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
    Keybind {
        id: &'a str,
        label: &'a str,
        key: &'a str,
        capturing: bool,
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
