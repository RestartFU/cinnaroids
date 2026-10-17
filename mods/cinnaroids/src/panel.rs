//! The ClickGUI's retained native surface and small live bindings, without rebuilding geometry.

use crate::state::{KeybindTarget, State};

/// Logical box the columns may be dragged within; the host fits it to small viewports.
pub const REFERENCE: [f32; 2] = [760.0, 420.0];
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
const MODULES: [(&str, usize, i32); 7] = [
    ("clicker", 0, 60),
    ("aim", 0, 92),
    ("reach", 0, 60),
    ("jump_reset", 0, 60),
    ("fake_lag", 1, 58),
    ("netherite", 2, 38),
    ("fullbright", 2, 0),
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
    json.push_str(r#"{"title":"Cinnaroids","toggle_key":""#);
    json.push_str(&p.menu_key);
    json.push_str(r#"","dark":true,"capture_key":"#);
    flag(json, state.capturing_key.is_some());
    json.push_str(r#","style":"compact","reference_size":[760,420],"motion":true,"controls":["#);
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
    json.push(']');
    json.push('}');
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

/// The muted trailing value of a module row.
fn value(json: &mut String, state: &State, module: usize) {
    let p = &state.preferences;
    json.push('"');
    match module {
        0 => {
            integer(json, p.cps.into());
            json.push_str(" CPS");
        }
        1 => {
            integer(json, p.aim_strength.into());
            json.push('%');
        }
        2 => {
            let tenths = (p.reach_blocks * 10.0).round() as i32;
            integer(json, tenths / 10);
            json.push('.');
            integer(json, tenths % 10);
            json.push_str(" blocks");
        }
        3 => {
            integer(json, p.jump_reset_window_ticks.into());
            json.push_str(" ticks");
        }
        4 => {
            integer(json, p.fake_lag_ms.into());
            json.push_str(" ms");
        }
        5 => {
            integer(json, p.netherite_range.into());
            json.push_str(" blocks");
        }
        _ => {}
    }
    json.push('"');
}

/// Write a small JSON object from trusted names, fixed copy, booleans and bounded numbers.
/// Direct appends avoid generic map/key escaping on every frame under the host fuel cap.
fn bindings_json(state: &State) -> String {
    let mut json = String::with_capacity(4096);
    json.push_str(r##"{"#world_blur":4,"#world_brightness":0.7,"#world_saturation":0.85"##);
    let enabled = state.enabled_flags();
    name(&mut json, "#menu_capture");
    integer(
        &mut json,
        (state.capturing_key == Some(KeybindTarget::Menu)).into(),
    );

    // Module rows, stacked per column with eased offsets beneath expanded settings.
    let mut heights = [0, 0, 0, MENU_BODY];
    for (index, (module, column, details)) in MODULES.iter().enumerate() {
        let on = enabled[index];
        let expanded = index < 6 && state.expanded[index];
        name_parts(&mut json, &[module, "_y"]);
        json.push_str("[0,");
        integer(&mut json, heights[*column]);
        json.push(']');
        name_parts(&mut json, &[module, "_bar"]);
        json.push_str(if on { "[1,1]" } else { "[1,0]" });
        name_parts(&mut json, &[module, "_indent"]);
        json.push_str(if on { "[13,6]" } else { "[10,6]" });
        if index < 6 {
            name_parts(&mut json, &[module, "_h"]);
            integer(&mut json, if expanded { *details } else { 0 });
            name_parts(&mut json, &[module, "_detail"]);
            integer(&mut json, expanded.into());
            name_parts(&mut json, &[module, "_angle"]);
            integer(&mut json, if expanded { 0 } else { -90 });
            name_parts(&mut json, &[module, "_value"]);
            value(&mut json, state, index);
        }
        heights[*column] += ROW + if expanded { *details } else { 0 };
    }
    for (index, target) in KEYED {
        let module = MODULES[index].0;
        let key = state.key_for(target);
        name_parts(&mut json, &[module, "_bind"]);
        text(&mut json, if key.is_empty() { "" } else { key_legend(key) });
        name_parts(&mut json, &[module, "_keycap"]);
        integer(&mut json, (!key.is_empty()).into());
        name_parts(&mut json, &[module, "_value_at"]);
        json.push_str(if key.is_empty() { "[72,6]" } else { "[50,6]" });
        name_parts(&mut json, &[module, "_key_color"]);
        json.push_str(if enabled[index] {
            "[1,0.5412,0.3961,1]"
        } else {
            "[0.902,0.9098,0.9216,1]"
        });
        name_parts(&mut json, &[module, "_capture"]);
        integer(&mut json, (state.capturing_key == Some(target)).into());
    }

    for (column, id) in COLUMNS.iter().enumerate() {
        let [x, y] = state.preferences.columns[column];
        let open = !state.preferences.collapsed[column];
        let body = if open { heights[column] } else { 0 };
        name_parts(&mut json, &["col_", id, "_pos"]);
        pair(&mut json, x, y);
        name_parts(&mut json, &["col_", id, "_pad"]);
        pair(&mut json, -x - PAD_HALF, -y - PAD_HALF);
        name_parts(&mut json, &["col_", id, "_tint_at"]);
        pair(&mut json, x + PAD_HALF, y + PAD_HALF);
        // Taller than the header while open, so the clip squares the seam, not the corners.
        name_parts(&mut json, &["col_", id, "_tint_h"]);
        integer(&mut json, if open { HEADER + 5 } else { HEADER });
        name_parts(&mut json, &["col_", id, "_in"]);
        json.push_str("[0,0]");
        name_parts(&mut json, &["col_", id, "_alpha"]);
        integer(&mut json, 1);
        name_parts(&mut json, &["col_", id, "_h"]);
        integer(&mut json, body);
        name_parts(&mut json, &["col_", id, "_bg"]);
        integer(&mut json, HEADER + body + if open { FOOTER } else { 0 });
        name_parts(&mut json, &["col_", id, "_open"]);
        integer(&mut json, open.into());
        name_parts(&mut json, &["col_", id, "_angle"]);
        integer(&mut json, if open { 0 } else { -90 });
        let lifted = state.drag.is_some_and(|drag| drag.column == column);
        name_parts(&mut json, &["col_", id, "_edge"]);
        json.push_str(if lifted { "[1,1,1,0.24]" } else { "[1,1,1,0]" });
        if column < 3 {
            let (on, all) = MODULES
                .iter()
                .enumerate()
                .filter(|(_, module)| module.1 == column)
                .fold((0, 0), |(on, all), (index, _)| {
                    (on + i32::from(enabled[index]), all + 1)
                });
            name_parts(&mut json, &["col_", id, "_count"]);
            json.push('"');
            integer(&mut json, on);
            json.push('/');
            integer(&mut json, all);
            json.push('"');
        }
    }
    json.push('}');
    json
}

#[cfg(test)]
mod tests;
