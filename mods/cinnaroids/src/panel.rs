//! Fold's retained native surface and small live bindings, without rebuilding JSON geometry.

use crate::state::{AimMode, State};

const MODULES: [(&str, i32); 7] = [
    ("clicker", 48),
    ("aim", 80),
    ("reach", 48),
    ("jump_reset", 48),
    ("fake_lag", 68),
    ("netherite", 48),
    ("fullbright", 0),
];

/// Copy pre-serialized geometry and append only the changing bindings within callback fuel.
pub(super) fn append(json: &mut String, state: &State) -> Result<(), serde_json::Error> {
    let surface = if state.preferences.dark_mode {
        include_str!("../ui/panel-dark.json")
    } else {
        include_str!("../ui/panel-light.json")
    };
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

/// Select the prevalidated theme and replace its live scalars without guest geometry copies.
pub(crate) fn route_update(state: &State) -> Result<String, serde_json::Error> {
    let mut json = String::with_capacity(1800);
    json.push_str(if state.preferences.dark_mode {
        r##"{"surface":"fold_dark","bindings":"##
    } else {
        r##"{"surface":"fold_light","bindings":"##
    });
    json.push_str(&bindings_json(state));
    json.push('}');
    Ok(json)
}

/// Append a trusted, compile-time binding name without repeatedly JSON-escaping it in WASM.
fn name(json: &mut String, key: &str) {
    json.push_str(",\"");
    json.push_str(key);
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

/// Append an integer scalar; all geometry is authored in whole GUI pixels.
fn number(json: &mut String, key: &str, value: i32) {
    name(json, key);
    integer(json, value);
}

/// Summaries contain bounded integers and fixed product copy, never arbitrary user text.
fn summary(json: &mut String, key: &str, value: u32, suffix: &str) {
    name(json, key);
    json.push('"');
    integer(json, value as i32);
    json.push_str(suffix);
    json.push('"');
}

/// Write a small JSON object from trusted names, fixed copy, booleans and bounded integers.
/// Direct appends avoid generic map/key escaping on every frame under the host fuel cap.
fn bindings_json(state: &State) -> String {
    let mut json = String::with_capacity(1600);
    json.push_str(r##"{"#menu_offset":[0,0],"#menu_alpha":1,"#menu_scale":1,"#menu_blur":0,"#settings_open":"##);
    json.push_str(if state.settings_open { "true" } else { "false" });
    let mut y = 0;
    const KEYS: [(&str, &str, &str); 7] = [
        (
            "#fold_clicker_offset",
            "#fold_clicker_height",
            "#fold_clicker_angle",
        ),
        ("#fold_aim_offset", "#fold_aim_height", "#fold_aim_angle"),
        (
            "#fold_reach_offset",
            "#fold_reach_height",
            "#fold_reach_angle",
        ),
        (
            "#fold_jump_reset_offset",
            "#fold_jump_reset_height",
            "#fold_jump_reset_angle",
        ),
        (
            "#fold_fake_lag_offset",
            "#fold_fake_lag_height",
            "#fold_fake_lag_angle",
        ),
        (
            "#fold_netherite_offset",
            "#fold_netherite_height",
            "#fold_netherite_angle",
        ),
        ("#fold_fullbright_offset", "", ""),
    ];
    for (index, ((_, height), (offset_key, height_key, angle_key))) in
        MODULES.iter().zip(KEYS).enumerate()
    {
        let expanded = state.expanded_module == Some(index);
        name(&mut json, offset_key);
        json.push_str("[0,");
        integer(&mut json, y);
        json.push(']');
        if index < 6 {
            number(&mut json, height_key, if expanded { *height } else { 0 });
            number(&mut json, angle_key, if expanded { 0 } else { -90 });
        }
        y += 32 + if expanded { *height } else { 0 };
    }
    number(&mut json, "#fold_content_height", y);
    let p = &state.preferences;
    summary(&mut json, "#summary_clicker", p.cps.into(), " CPS");
    summary(
        &mut json,
        "#summary_aim",
        p.aim_strength.into(),
        if p.aim_mode == AimMode::WhileClicking {
            "% · While clicking"
        } else {
            "% · Continuous"
        },
    );
    let reach = (p.reach_blocks * 10.0).round() as u32;
    name(&mut json, "#summary_reach");
    json.push('"');
    integer(&mut json, (reach / 10) as i32);
    json.push('.');
    integer(&mut json, (reach % 10) as i32);
    json.push_str(" blocks\"");
    summary(
        &mut json,
        "#summary_jump_reset",
        p.jump_reset_window_ticks.into(),
        " ticks",
    );
    summary(&mut json, "#summary_fake_lag", p.fake_lag_ms.into(), " ms");
    summary(
        &mut json,
        "#summary_netherite",
        p.netherite_range.into(),
        " blocks",
    );
    name(&mut json, "#summary_fullbright");
    json.push_str("\"No extra settings\"");
    let enabled = [
        state.modules.clicker,
        state.modules.aim,
        state.modules.reach,
        state.modules.jump_reset,
        state.modules.fake_lag,
        state.modules.netherite,
        state.modules.fullbright,
    ]
    .into_iter()
    .filter(|on| *on)
    .count();
    summary(&mut json, "#enabled_count", enabled as u32, " of 7 enabled");
    name(&mut json, "#binding_hint");
    json.push_str(if state.capturing_key.is_some() {
        "\"Press a key. Escape clears a module binding; cancels Menu.\""
    } else {
        "\"Select a key binding to change it. Escape clears it.\""
    });
    json.push('}');
    json
}

#[cfg(test)]
mod tests;
