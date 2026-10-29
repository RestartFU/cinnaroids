//! The ClickGUI's retained native surface and small live bindings, without rebuilding geometry.
//!
//! Cinnabar routes the panel's value-bearing controls but no longer formats them, so this
//! module also publishes each control's feedback: `#c<index>_on` toggle progress,
//! `#c<index>_text` values and `#c<index>_fill` slider fractions. Hover and press tints are
//! native button states in the document.

use crate::state::{KeybindTarget, State};

/// Logical box the columns may be dragged within; the host fits it to small viewports.
pub const REFERENCE: [f32; 2] = [760.0, 420.0];
/// Straight RGBA of Cinnabar's popup widgets, the slider value editor and the dropdown, as
/// `{background, surface, text, accent, border}`: the dark compact palette Cinnabar drew this
/// panel's popups with before panels lost `dark` and `style`.
pub const WIDGET_COLORS: [[f32; 4]; 5] = [
    [0.075, 0.09, 0.115, 0.96],
    [0.16, 0.185, 0.22, 0.98],
    [0.94, 0.95, 0.97, 1.0],
    [1.0, 0.34, 0.32, 1.0],
    [0.22, 0.26, 0.31, 0.9],
];
/// [`WIDGET_COLORS`] as the shell's static `widget_colors` member.
const WIDGET_COLORS_JSON: &str = concat!(
    r#""widget_colors":{"background":[0.075,0.09,0.115,0.96],"surface":[0.16,0.185,0.22,0.98],"#,
    r#""text":[0.94,0.95,0.97,1],"accent":[1,0.34,0.32,1],"border":[0.22,0.26,0.31,0.9]}"#,
);
pub const COLUMN_WIDTH: f32 = 150.0;
const HEADER: i32 = 22;
const ROW: i32 = 20;
const FOOTER: i32 = 5;
/// Half the pointer pad's extent; pad sliders report pointer positions from the box centre.
pub const PAD_HALF: f32 = 2048.0;
pub const COLUMNS: [&str; 4] = ["combat", "network", "render", "menu"];
/// Default column origins relative to the reference box centre.
pub const DEFAULT_COLUMNS: [[f32; 2]; 4] = [
    [-380.0, -166.0],
    [-177.0, -166.0],
    [27.0, -166.0],
    [230.0, -166.0],
];
const MENU_BODY: i32 = 52;

/// Module binding prefix, owning column and the height of its generated settings region.
/// A column's first module never moves and so has no `_y` binding.
const MODULES: [(&str, usize, i32); 9] = [
    ("clicker", 0, 60),
    ("aim", 0, 112),
    ("reach", 0, 60),
    ("jump_reset", 0, 60),
    ("fake_lag", 1, 58),
    ("netherite", 2, 38),
    ("fullbright", 2, 0),
    ("player_esp", 2, 130),
    ("auto_totem", 0, 0),
];
const KEYED: [(usize, KeybindTarget); 4] = [
    (0, KeybindTarget::Clicker),
    (1, KeybindTarget::Aim),
    (2, KeybindTarget::Reach),
    (3, KeybindTarget::JumpReset),
];

/// Copy pre-serialized geometry and append only the changing bindings within callback fuel.
pub(super) fn append(json: &mut String, state: &State) -> Result<(), serde_json::Error> {
    let surface = include_str!("../ui/panel.json");
    json.pop();
    json.push_str(",\"surface\":");
    json.push_str(&surface[..surface.len() - 1]);
    json.push_str(",\"bindings\":");
    json.push_str(&bindings_json(state));
    json.push_str("}}");
    Ok(())
}

/// Stage a minimal valid surface; the same callback replaces it from the embedded catalog.
/// Keeping the reference size here preserves native fitting and the current menu shortcut.
/// Panels require a surface; the native title, theme and layout fields no longer exist.
pub(crate) fn append_shell(json: &mut String) {
    json.pop();
    json.push_str(r#","surface":{"screen":"cinnaroids.panel","document":"{\"namespace\":\"cinnaroids\",\"panel\":{\"type\":\"panel\"}}"}}"#);
}

/// Append the control shell from static fragments: the same schema `State::panel_json`
/// serializes with serde, without per-byte escaping or float formatting in a frame callback.
pub(crate) fn shell(json: &mut String, state: &State) {
    let p = &state.preferences;
    let m = &state.modules;
    let capturing = |target| state.capturing_key == Some(target);
    json.push_str(r#"{"toggle_key":""#);
    json.push_str(&p.menu_key);
    json.push_str(r#"","capture_key":"#);
    flag(json, state.capturing_key.is_some());
    json.push_str(r#","reference_size":[760,420],"motion":true,"#);
    json.push_str(WIDGET_COLORS_JSON);
    json.push_str(r#","controls":["#);
    keybind(
        json,
        "menu_key",
        "Menu key",
        &p.menu_key,
        capturing(KeybindTarget::Menu),
    );
    toggle(json, "clicker", "Clicker", m.clicker);
    slider(
        json,
        r#""cps","label":"CPS""#,
        f32::from(p.cps),
        "1,\"max\":30,\"step\":1",
    );
    keybind(
        json,
        "clicker_key",
        "Clicker key",
        &p.clicker_key,
        capturing(KeybindTarget::Clicker),
    );
    toggle(json, "aim", "Aim assist", m.aim);
    slider(
        json,
        r#""aim_strength","label":"Aim strength""#,
        f32::from(p.aim_strength),
        "0,\"max\":100,\"step\":1",
    );
    json.push_str(r#",{"kind":"choice","id":"aim_mode","label":"Mode","index":"#);
    json.push(if p.aim_mode == crate::state::AimMode::WhileClicking {
        '1'
    } else {
        '0'
    });
    json.push_str(r#","options":["Continuous","While clicking"]}"#);
    keybind(
        json,
        "aim_key",
        "Aim assist key",
        &p.aim_key,
        capturing(KeybindTarget::Aim),
    );
    toggle(json, "reach", "Reach", m.reach);
    slider(
        json,
        r#""reach_blocks","label":"Reach distance""#,
        (p.reach_blocks * 10.0).round() / 10.0,
        "3,\"max\":6,\"step\":0.1",
    );
    keybind(
        json,
        "reach_key",
        "Reach key",
        &p.reach_key,
        capturing(KeybindTarget::Reach),
    );
    toggle(json, "jump_reset", "Auto Jump Reset", m.jump_reset);
    slider(
        json,
        r#""jump_reset_window_ticks","label":"Hit window (ticks)""#,
        f32::from(p.jump_reset_window_ticks),
        "1,\"max\":10,\"step\":1",
    );
    keybind(
        json,
        "jump_reset_key",
        "Jump reset key",
        &p.jump_reset_key,
        capturing(KeybindTarget::JumpReset),
    );
    toggle(json, "fake_lag", "FakeLag", m.fake_lag);
    slider(
        json,
        r#""fake_lag_ms","label":"FakeLag delay (ms)""#,
        f32::from(p.fake_lag_ms),
        "0,\"max\":1000,\"step\":1",
    );
    toggle(
        json,
        "show_real_position",
        "Show real position",
        p.show_real_position,
    );
    toggle(json, "netherite", "Netherite Finder", m.netherite);
    slider(
        json,
        r#""netherite_range","label":"Finder range""#,
        f32::from(p.netherite_range),
        "1,\"max\":128,\"step\":1",
    );
    toggle(json, "fullbright", "Fullbright", m.fullbright);
    json.push_str(concat!(
        r#",{"kind":"button","id":"expand_clicker","label":"Clicker settings"}"#,
        r#",{"kind":"button","id":"expand_aim","label":"Aim assist settings"}"#,
        r#",{"kind":"button","id":"expand_reach","label":"Reach settings"}"#,
        r#",{"kind":"button","id":"expand_jump_reset","label":"Auto Jump Reset settings"}"#,
        r#",{"kind":"button","id":"expand_fake_lag","label":"FakeLag settings"}"#,
        r#",{"kind":"button","id":"expand_netherite","label":"Netherite Finder settings"}"#,
        r#",{"kind":"button","id":"reset_layout","label":"Reset layout"}"#,
    ));
    slider(
        json,
        r#""drag_x","label":"Column drag x""#,
        state.pad_value(0),
        "-2048,\"max\":2048,\"step\":0.5",
    );
    slider(
        json,
        r#""drag_y","label":"Column drag y""#,
        state.pad_value(1),
        "-2048,\"max\":2048,\"step\":0.5",
    );
    toggle(
        json,
        "aim_horizontal_only",
        "Horizontal only",
        p.aim_horizontal_only,
    );
    toggle(json, "player_esp", "Player ESP", m.player_esp);
    slider(
        json,
        r#""esp_range","label":"ESP range""#,
        f32::from(p.esp_range),
        "1,\"max\":128,\"step\":1",
    );
    json.push_str(r#",{"kind":"choice","id":"esp_color","label":"Outline color","index":"#);
    integer(json, i32::from(p.esp_color));
    json.push_str(r#","options":["Coral","Pink","Cyan","White"]}"#);
    toggle(json, "esp_names", "Names", p.esp_names);
    toggle(json, "esp_distance", "Distance", p.esp_distance);
    toggle(json, "esp_health", "Health", p.esp_health);
    json.push_str(r#",{"kind":"button","id":"expand_esp","label":"Player ESP settings"}"#);
    toggle(json, "auto_totem", "AutoTotem", m.auto_totem);
    json.push_str("] }");
    append_shell(json);
}

fn flag(json: &mut String, value: bool) {
    json.push_str(if value { "true" } else { "false" });
}

fn toggle(json: &mut String, id: &str, label: &str, value: bool) {
    if !json.ends_with('[') {
        json.push(',');
    }
    json.push_str(r#"{"kind":"toggle","id":""#);
    json.push_str(id);
    json.push_str(r#"","label":""#);
    json.push_str(label);
    json.push_str(r#"","value":"#);
    flag(json, value);
    json.push('}');
}

/// `head` is the quoted ID and label pair; `bounds` continues after `"min":`.
fn slider(json: &mut String, head: &str, value: f32, bounds: &str) {
    json.push_str(r#",{"kind":"slider","id":"#);
    json.push_str(head);
    json.push_str(r#","value":"#);
    decimal(json, value);
    json.push_str(r#","min":"#);
    json.push_str(bounds);
    json.push('}');
}

/// Key legends are validated alphanumeric key names or `None`, so they need no escaping.
fn keybind(json: &mut String, id: &str, label: &str, key: &str, capturing: bool) {
    if !json.ends_with('[') {
        json.push(',');
    }
    json.push_str(r#"{"kind":"keybind","id":""#);
    json.push_str(id);
    json.push_str(r#"","label":""#);
    json.push_str(label);
    json.push_str(r#"","key":""#);
    json.push_str(key_legend(key));
    json.push_str(r#"","capturing":"#);
    flag(json, capturing);
    json.push('}');
}

/// Select the prevalidated route and replace its live scalars without guest geometry copies.
pub(crate) fn route_update(state: &State) -> Result<String, serde_json::Error> {
    let mut json = String::with_capacity(4096);
    json.push_str(r##"{"surface":"clickgui","bindings":"##);
    json.push_str(&bindings_json(state));
    json.push('}');
    Ok(json)
}

/// Updates live control values without replacing the retained surface or its transitions.
/// Publish these ahead of incremental visual bindings so key capture stays responsive.
#[cfg(any(target_arch = "wasm32", test))]
pub(crate) fn control_patch(state: &State) -> String {
    let mut json = String::with_capacity(4096);
    json.push_str(r#"{"capture_key":"#);
    flag(&mut json, state.capturing_key.is_some());
    json.push_str(r#","control_values":["#);
    let mut first = true;
    for (index, value) in toggles(state) {
        if !first {
            json.push(',');
        }
        first = false;
        json.push('[');
        integer(&mut json, index as i32);
        json.push(',');
        flag(&mut json, value);
        json.push(']');
    }
    for (index, value, _, _) in sliders(state).into_iter().chain([
        (27, state.pad_value(0), -2048.0, 2048.0),
        (28, state.pad_value(1), -2048.0, 2048.0),
    ]) {
        json.push_str(",[");
        integer(&mut json, index as i32);
        json.push(',');
        decimal(&mut json, value);
        json.push(']');
    }
    for (index, value) in [
        (
            6,
            i32::from(state.preferences.aim_mode == crate::state::AimMode::WhileClicking),
        ),
        (32, i32::from(state.preferences.esp_color)),
    ] {
        json.push_str(",[");
        integer(&mut json, index);
        json.push(',');
        integer(&mut json, value);
        json.push(']');
    }
    json.push_str(r#"],"controls":["#);
    for (position, (_, target)) in KEYBINDS.iter().enumerate() {
        if position != 0 {
            json.push(',');
        }
        let id = match target {
            KeybindTarget::Menu => "menu_key",
            KeybindTarget::Clicker => "clicker_key",
            KeybindTarget::Aim => "aim_key",
            KeybindTarget::Reach => "reach_key",
            KeybindTarget::JumpReset => "jump_reset_key",
        };
        json.push_str(r#"{"id":"#);
        text(&mut json, id);
        json.push_str(r#","key":"#);
        text(&mut json, key_legend(state.key_for(*target)));
        json.push_str(r#","capturing":"#);
        flag(&mut json, state.capturing_key == Some(*target));
        json.push('}');
    }
    json.push_str("]}");
    json
}

/// Appends completed bindings to either an empty value patch or an initial route selection.
#[cfg(any(target_arch = "wasm32", test))]
pub(crate) fn with_bindings(mut patch: String, bindings: &str) -> String {
    patch.pop();
    if patch.len() > 1 {
        patch.push(',');
    }
    patch.push_str("\"bindings\":");
    patch.push_str(bindings);
    patch.push('}');
    patch
}

/// Short keycap legend for a physical key name, such as `R` for `KeyR`.
pub fn key_legend(key: &str) -> &str {
    if key.is_empty() {
        return "None";
    }
    key.strip_prefix("Key")
        .or_else(|| key.strip_prefix("Digit"))
        .filter(|rest| !rest.is_empty())
        .unwrap_or(key)
}

/// Append a trusted, compile-time binding name without repeatedly JSON-escaping it in WASM.
fn name(json: &mut String, key: &str) {
    json.push_str(",\"");
    json.push_str(key);
    json.push_str("\":");
}

/// Append a binding name assembled from trusted parts, such as `#col_` `combat` `_pos`.
fn name_parts(json: &mut String, parts: &[&str]) {
    json.push_str(",\"#");
    for part in parts {
        json.push_str(part);
    }
    json.push_str("\":");
}

/// Append bounded decimal digits without allocator-backed general formatting in a frame callback.
fn integer(json: &mut String, value: i32) {
    if value < 0 {
        json.push('-');
    }
    let mut remaining = value.unsigned_abs();
    let mut digits = [0u8; 10];
    let mut start = digits.len();
    loop {
        start -= 1;
        digits[start] = b'0' + (remaining % 10) as u8;
        remaining /= 10;
        if remaining == 0 {
            break;
        }
    }
    for digit in &digits[start..] {
        json.push(char::from(*digit));
    }
}

/// Append a coordinate with one decimal place; positions move in half logical units.
fn decimal(json: &mut String, value: f32) {
    let tenths = (value * 10.0).round() as i32;
    if tenths < 0 {
        json.push('-');
    }
    integer(json, tenths.abs() / 10);
    let fraction = tenths.abs() % 10;
    if fraction != 0 {
        json.push('.');
        integer(json, fraction);
    }
}

fn pair(json: &mut String, x: f32, y: f32) {
    json.push('[');
    decimal(json, x);
    json.push(',');
    decimal(json, y);
    json.push(']');
}

/// Bounded product copy and key legends are trusted ASCII without quotes or escapes.
fn text(json: &mut String, value: &str) {
    json.push('"');
    json.push_str(value);
    json.push('"');
}

/// Toggle controls by panel index; one eased progress drives each row wash, lift bar and switch.
fn toggles(state: &State) -> [(usize, bool); 14] {
    let (m, p) = (&state.modules, &state.preferences);
    [
        (1, m.clicker),
        (4, m.aim),
        (8, m.reach),
        (11, m.jump_reset),
        (14, m.fake_lag),
        (16, p.show_real_position),
        (17, m.netherite),
        (19, m.fullbright),
        (29, p.aim_horizontal_only),
        (30, m.player_esp),
        (33, p.esp_names),
        (34, p.esp_distance),
        (35, p.esp_health),
        (37, m.auto_totem),
    ]
}

/// Visible sliders by panel index with their value and range; the drag pads have no visuals.
fn sliders(state: &State) -> [(usize, f32, f32, f32); 7] {
    let p = &state.preferences;
    [
        (2, f32::from(p.cps), 1.0, 30.0),
        (5, f32::from(p.aim_strength), 0.0, 100.0),
        (
            9,
            (p.reach_blocks * 10.0).round() / 10.0,
            3.0,
            mod_api::MAX_ENTITY_REACH_BLOCKS,
        ),
        (12, f32::from(p.jump_reset_window_ticks), 1.0, 10.0),
        (
            15,
            f32::from(p.fake_lag_ms),
            0.0,
            mod_api::MAX_PACKET_DELAY_MS as f32,
        ),
        (
            18,
            f32::from(p.netherite_range),
            1.0,
            crate::finder::MAX_RANGE,
        ),
        (31, f32::from(p.esp_range), 1.0, crate::esp::MAX_RANGE),
    ]
}

/// Keybind controls by panel index.
const KEYBINDS: [(usize, KeybindTarget); 5] = [
    (0, KeybindTarget::Menu),
    (3, KeybindTarget::Clicker),
    (7, KeybindTarget::Aim),
    (10, KeybindTarget::Reach),
    (13, KeybindTarget::JumpReset),
];

/// The selected option of each choice control, by panel index.
fn choices(state: &State) -> [(usize, &'static str); 2] {
    let p = &state.preferences;
    [
        (
            6,
            if p.aim_mode == crate::state::AimMode::WhileClicking {
                "While clicking"
            } else {
                "Continuous"
            },
        ),
        (
            32,
            crate::esp::COLOR_NAMES
                [usize::from(p.esp_color).min(crate::esp::COLOR_NAMES.len() - 1)],
        ),
    ]
}

/// A slider value as Cinnabar's controls showed it: at most two decimals, trailing zeros dropped.
fn slider_text(json: &mut String, value: f32) {
    let hundredths = (value * 100.0).round() as i32;
    json.push('"');
    if hundredths < 0 {
        json.push('-');
    }
    let magnitude = hundredths.unsigned_abs() as i32;
    integer(json, magnitude / 100);
    let fraction = magnitude % 100;
    if fraction != 0 {
        json.push('.');
        json.push(char::from(b'0' + (fraction / 10) as u8));
        if fraction % 10 != 0 {
            json.push(char::from(b'0' + (fraction % 10) as u8));
        }
    }
    json.push('"');
}

/// A bounded 0–1 fraction with four decimals, without general float formatting.
fn fraction(json: &mut String, value: f32) {
    let units = (value.clamp(0.0, 1.0) * 10_000.0).round() as i32;
    integer(json, units / 10_000);
    let rest = units % 10_000;
    if rest != 0 {
        json.push('.');
        let mut digits = [b'0'; 4];
        let mut remaining = rest;
        for digit in digits.iter_mut().rev() {
            *digit = b'0' + (remaining % 10) as u8;
            remaining /= 10;
        }
        let end = digits.iter().rposition(|digit| *digit != b'0').unwrap() + 1;
        for digit in &digits[..end] {
            json.push(char::from(*digit));
        }
    }
}

/// Key names are validated ASCII alphanumerics, so only symbol legends need escaping.
fn escaped(json: &mut String, value: &str) {
    json.push('"');
    for character in value.chars() {
        if matches!(character, '"' | '\\') {
            json.push('\\');
        }
        json.push(character);
    }
    json.push('"');
}

/// Short keycap label for a physical key, as Cinnabar's native keybind controls showed it.
pub fn key_label(key: &str) -> &str {
    let key = key_legend(key);
    match key {
        "ControlLeft" => "LCtrl",
        "ControlRight" => "RCtrl",
        "ShiftLeft" => "LShift",
        "ShiftRight" => "RShift",
        "AltLeft" => "LAlt",
        "AltRight" => "RAlt",
        "SuperLeft" | "MetaLeft" => "LWin",
        "SuperRight" | "MetaRight" => "RWin",
        "ArrowLeft" => "Left",
        "ArrowRight" => "Right",
        "ArrowUp" => "Up",
        "ArrowDown" => "Down",
        "Backspace" => "Bksp",
        "CapsLock" => "Caps",
        "Escape" => "Esc",
        "PageUp" => "PgUp",
        "PageDown" => "PgDn",
        "PrintScreen" => "PrtSc",
        "ScrollLock" => "ScrLk",
        "NumLock" => "NumLk",
        "ContextMenu" => "Menu",
        "Delete" => "Del",
        "Insert" => "Ins",
        "Backquote" => "`",
        "Backslash" => "\\",
        "BracketLeft" => "[",
        "BracketRight" => "]",
        "Comma" => ",",
        "Period" => ".",
        "Slash" => "/",
        "Semicolon" => ";",
        "Quote" => "'",
        "Minus" => "-",
        "Equal" => "=",
        "IntlBackslash" => "Intl\\",
        "NumpadAdd" => "Num+",
        "NumpadSubtract" => "Num-",
        "NumpadMultiply" => "Num*",
        "NumpadDivide" => "Num/",
        "NumpadDecimal" => "Num.",
        "NumpadEnter" => "NumEnt",
        "NumpadEqual" => "Num=",
        "NumpadComma" => "Num,",
        "NumpadHash" => "Num#",
        "NumpadParenLeft" => "Num(",
        "NumpadParenRight" => "Num)",
        "NumpadBackspace" => "NumBk",
        "NumpadMemoryAdd" => "NumM+",
        "NumpadMemorySubtract" => "NumM-",
        "NumpadMemoryStore" => "NumMS",
        "NumpadMemoryRecall" => "NumMR",
        "NumpadMemoryClear" => "NumMC",
        "NumpadClear" => "NumClr",
        "NumpadClearEntry" => "NumCE",
        "AudioVolumeMute" => "Mute",
        "AudioVolumeUp" => "Vol+",
        "AudioVolumeDown" => "Vol-",
        "MediaPlayPause" => "Play",
        "MediaStop" | "BrowserStop" => "Stop",
        "MediaTrackNext" => "Next",
        "MediaTrackPrevious" => "Prev",
        "LaunchMail" => "Mail",
        "LaunchApp1" => "App1",
        "LaunchApp2" => "App2",
        "MediaSelect" => "Media",
        "BrowserBack" => "Back",
        "BrowserForward" => "Fwd",
        "BrowserRefresh" => "Reload",
        "BrowserSearch" => "Search",
        "BrowserFavorites" => "Fav",
        "BrowserHome" => "Home",
        // Numpad digits read as `Num7`; the static table keeps this allocation-free.
        "Numpad0" => "Num0",
        "Numpad1" => "Num1",
        "Numpad2" => "Num2",
        "Numpad3" => "Num3",
        "Numpad4" => "Num4",
        "Numpad5" => "Num5",
        "Numpad6" => "Num6",
        "Numpad7" => "Num7",
        "Numpad8" => "Num8",
        "Numpad9" => "Num9",
        other => other,
    }
}

/// Write a small JSON object from trusted names, fixed copy, booleans and bounded numbers.
/// Direct appends avoid generic map/key escaping on every frame under the host fuel cap.
/// A snapshot of panel scalars serialized one bounded row at a time.
pub(crate) struct Bindings {
    state: State,
    json: String,
    heights: [i32; 4],
    placed: [bool; 3],
    step: usize,
}
impl Bindings {
    /// Captures one coherent update without retaining a borrow of gameplay state.
    pub(crate) fn new(state: &State) -> Self {
        Self {
            state: state.clone(),
            json: String::with_capacity(8192),
            heights: [0, 0, 0, MENU_BODY],
            placed: [false; 3],
            step: 0,
        }
    }
    /// The last binding step also publishes the completed string to the pending patch.
    #[cfg(any(target_arch = "wasm32", test))]
    pub(crate) fn finishing(&self) -> bool {
        self.step == 43
    }

    /// Appends at most one module, column or feedback control; true after the closing brace.
    pub(crate) fn advance(&mut self) -> bool {
        let state = &self.state;
        let enabled = state.enabled_flags();
        let json = &mut self.json;
        let heights = &mut self.heights;
        let placed = &mut self.placed;
        match self.step {
            0 => {
                json.push_str(
                    r##"{"#world_blur":4,"#world_brightness":0.7,"#world_saturation":0.85"##,
                );
                name(json, "#menu_capture");
                integer(
                    json,
                    (state.capturing_key == Some(KeybindTarget::Menu)).into(),
                );
            }
            1..=9 => {
                let index = self.step - 1;
                let (module, column, details) = &MODULES[index];

                let reason = &state.access.0[index];
                name_parts(json, &[module, "_available"]);
                flag(json, reason.is_empty());
                name_parts(json, &[module, "_blocked"]);
                flag(json, !reason.is_empty());
                name_parts(json, &[module, "_opacity"]);
                json.push_str(if reason.is_empty() { "1" } else { "0.4" });
                name_parts(json, &[module, "_permission"]);
                escaped(json, reason);
                let on = enabled[index];
                let expanded = *details > 0 && state.expanded[index];
                if std::mem::replace(&mut placed[*column], true) {
                    name_parts(json, &[module, "_y"]);
                    json.push_str("[0,");
                    integer(json, heights[*column]);
                    json.push(']');
                }
                name_parts(json, &[module, "_bar"]);
                json.push_str(if on { "[1,1]" } else { "[1,0]" });
                name_parts(json, &[module, "_indent"]);
                json.push_str(if on { "[13,6]" } else { "[10,6]" });
                if *details > 0 {
                    name_parts(json, &[module, "_h"]);
                    integer(json, if expanded { *details } else { 0 });
                    name_parts(json, &[module, "_detail"]);
                    integer(json, expanded.into());
                    name_parts(json, &[module, "_angle"]);
                    integer(json, if expanded { 0 } else { -90 });
                }
                heights[*column] += ROW + if expanded { *details } else { 0 };
            }
            10 => {
                // Other rows reuse their first slider's text; Reach keeps one fixed decimal.
                let tenths = (state.preferences.reach_blocks * 10.0).round() as i32;
                name(json, "#reach_value");
                json.push('"');
                integer(json, tenths / 10);
                json.push('.');
                integer(json, tenths % 10);
                json.push_str(" blocks\"");
                for (index, target) in KEYED {
                    let module = MODULES[index].0;
                    let bound = !state.key_for(target).is_empty();
                    name_parts(json, &[module, "_keycap"]);
                    integer(json, bound.into());
                    // The trailing value slides left to make room for the keycap.
                    name_parts(json, &[module, "_value_at"]);
                    json.push_str(if bound { "[50,6]" } else { "[72,6]" });
                    name_parts(json, &[module, "_capture"]);
                    integer(json, (state.capturing_key == Some(target)).into());
                }
            }
            11..=14 => {
                let column = self.step - 11;
                let id = COLUMNS[column];

                let [x, y] = state.preferences.columns[column];
                let open = !state.preferences.collapsed[column];
                let body = if open { heights[column] } else { 0 };
                name_parts(json, &["col_", id, "_pos"]);
                pair(json, x, y);
                // The drag pad is centred on the desk origin from inside the moving header.
                name_parts(json, &["col_", id, "_pad"]);
                pair(json, -x, -y);
                name_parts(json, &["col_", id, "_in"]);
                json.push_str("[0,0]");
                name_parts(json, &["col_", id, "_alpha"]);
                integer(json, 1);
                name_parts(json, &["col_", id, "_h"]);
                integer(json, body);
                name_parts(json, &["col_", id, "_bg"]);
                integer(json, HEADER + body + if open { FOOTER } else { 0 });
                name_parts(json, &["col_", id, "_open"]);
                integer(json, open.into());
                name_parts(json, &["col_", id, "_angle"]);
                integer(json, if open { 0 } else { -90 });
                let lifted = state.drag.is_some_and(|drag| drag.column == column);
                name_parts(json, &["col_", id, "_edge"]);
                json.push_str(if lifted { "[1,1,1,0.24]" } else { "[1,1,1,0]" });
                if column < 3 {
                    let (on, all) = MODULES
                        .iter()
                        .enumerate()
                        .filter(|(_, module)| module.1 == column)
                        .fold((0, 0), |(on, all), (index, _)| {
                            (on + i32::from(enabled[index]), all + 1)
                        });
                    name_parts(json, &["col_", id, "_count"]);
                    json.push('"');
                    integer(json, on);
                    json.push('/');
                    integer(json, all);
                    json.push('"');
                }
            }
            15..=28 => {
                let (index, on) = toggles(state)[self.step - 15];
                control(json, index, "_on");
                integer(json, on.into());
            }
            29..=35 => {
                let (index, value, min, max) = sliders(state)[self.step - 29];
                control(json, index, "_text");
                slider_text(json, value);
                control(json, index, "_fill");
                fraction(json, (value - min) / (max - min));
            }
            36..=40 => {
                let (index, target) = KEYBINDS[self.step - 36];
                control(json, index, "_text");
                escaped(json, key_label(state.key_for(target)));
            }
            41..=42 => {
                let (index, option) = choices(state)[self.step - 41];
                control(json, index, "_text");
                text(json, option);
            }
            43 => json.push('}'),
            _ => return true,
        }
        self.step += 1;
        self.step == 44
    }
    /// The completed scalar object, ready for one bounded publication call.
    pub(crate) fn finish(self) -> String {
        self.json
    }
}

/// Names a bounded two-digit control without allocating a temporary formatted integer.
fn control(json: &mut String, index: usize, suffix: &str) {
    let digits = [b'0' + (index / 10) as u8, b'0' + (index % 10) as u8];
    let digits = if index < 10 {
        &digits[1..]
    } else {
        &digits[..]
    };
    name_parts(
        json,
        &[
            "c",
            std::str::from_utf8(digits).expect("control digits"),
            suffix,
        ],
    );
}

/// Builds the same scalar object synchronously for offline schema validation.
fn bindings_json(state: &State) -> String {
    let mut bindings = Bindings::new(state);
    while !bindings.advance() {}
    bindings.finish()
}

#[cfg(test)]
mod tests;
