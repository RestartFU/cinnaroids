//! Bounded actor silhouettes with native mask composition and compact HUD labels.

use crate::state::Preferences;
use mod_api::bindings::cinnabar::{
    extension::player_state::Actor,
    world_draw::masks::{Rgba, Style},
};

/// Leaves callback fuel for combat assistance and maximum-size host observations.
pub const MAX_PLAYERS: usize = 6;
pub const MAX_RANGE: f32 = 128.;
pub const COLOR_NAMES: &[&str] = &["Coral", "Pink", "Cyan", "White"];
const COLORS: [[f32; 4]; 4] = [
    [1., 0.5412, 0.3961, 1.],
    [1., 0.12, 0.55, 1.],
    [0.2, 0.85, 1., 1.],
    [0.95, 0.95, 0.95, 1.],
];
/// One callback's selected actor identities and camera-projected HUD labels.
#[derive(Default)]
pub struct Output {
    pub players: Vec<u64>,
    pub labels: String,
}

/// Draws only current-session remote players; absent bounds and health stay unknown.
pub fn build(
    p: &Preferences,
    session: u64,
    dimension: i32,
    eye: [f32; 3],
    actors: &[Actor],
) -> Output {
    let mut output = Output {
        players: Vec::with_capacity(MAX_PLAYERS),
        labels: String::with_capacity(MAX_PLAYERS * 256),
    };
    output.labels.push_str("{\"cards\":[],\"world_labels\":[");
    let mut labels = 0;
    let mut players = 0;
    for actor in actors.iter().take(mod_api::MAX_OBSERVED_ACTORS) {
        if !actor.player
            || actor.session != session
            || actor.dimension != dimension
            || actor.runtime_id == 0
        {
            continue;
        }
        if players == MAX_PLAYERS {
            break;
        }
        let position = [actor.position.0, actor.position.1, actor.position.2];
        if !eye.iter().chain(&position).all(|n| n.is_finite()) {
            continue;
        }
        let distance = position
            .iter()
            .zip(eye)
            .map(|(a, b)| (a - b).powi(2))
            .sum::<f32>()
            .sqrt();
        if distance > f32::from(p.esp_range) {
            continue;
        }
        let Some((min, max)) = bounds(actor) else {
            continue;
        };
        // Exclude the camera actor, including giant/custom actor bounds.
        if (0..3).all(|i| eye[i] >= min[i] && eye[i] <= max[i]) {
            continue;
        }
        players += 1;
        output.players.push(actor.runtime_id);
        let mut text = String::with_capacity(actor.name.len() + 32);
        if p.esp_names {
            text.push_str(&actor.name);
        }
        if p.esp_distance {
            if !text.is_empty() {
                text.push('\n');
            }
            integer(&mut text, distance.round() as i32);
            text.push_str(" blocks");
        }
        if p.esp_health
            && let Some(health) = actor.health.filter(|n| n.is_finite() && *n >= 0.)
        {
            if !text.is_empty() {
                text.push_str(if p.esp_distance { " · " } else { "\n" });
            }
            integer(&mut text, health.ceil().min(i32::MAX as f32) as i32);
            if let Some(maximum) = actor.max_health.filter(|n| n.is_finite() && *n > 0.) {
                text.push('/');
                integer(&mut text, maximum.ceil().min(i32::MAX as f32) as i32);
            }
            text.push_str(" HP");
        }
        if !text.is_empty() {
            if labels > 0 {
                output.labels.push(',');
            }
            labels += 1;
            output.labels.push_str("{\"position\":[");
            for (i, value) in [
                (min[0] + max[0]) * 0.5,
                max[1] + 0.65,
                (min[2] + max[2]) * 0.5,
            ]
            .into_iter()
            .enumerate()
            {
                if i > 0 {
                    output.labels.push(',');
                }
                coordinate(&mut output.labels, value);
            }
            output.labels.push_str("],\"text\":");
            // Names are untrusted server text, so always use JSON string escaping.
            quoted(&mut output.labels, &text);
            output.labels.push_str(
                ",\"scale\":1.2,\"padding\":2,\"shadow\":false,\"background_opacity\":0.65}",
            );
        }
    }
    output.labels.push_str("]}");
    output
}

/// Unions valid native hit boxes without fabricating standing dimensions.
fn bounds(actor: &Actor) -> Option<([f32; 3], [f32; 3])> {
    let mut lo = [f32::INFINITY; 3];
    let mut hi = [f32::NEG_INFINITY; 3];
    for b in actor.boxes.iter().take(mod_api::MAX_OBSERVED_ACTOR_BOXES) {
        let min = [b.min.0, b.min.1, b.min.2];
        let max = [b.max.0, b.max.1, b.max.2];
        if !(0..3).all(|i| {
            min[i].is_finite()
                && max[i].is_finite()
                && min[i] <= max[i]
                && min[i].abs().max(max[i].abs()) <= mod_api::MAX_PRIMITIVE_COORDINATE
        }) {
            continue;
        }
        for i in 0..3 {
            lo[i] = lo[i].min(min[i]);
            hi[i] = hi[i].max(max[i]);
        }
    }
    (0..3).all(|i| lo[i] < hi[i]).then_some((lo, hi))
}

/// A sharp physical-pixel edge and a restrained halo follow the actual animated mesh.
pub fn appearance(color: u8) -> Style {
    let [r, g, b, a] = COLORS[usize::from(color).min(COLORS.len() - 1)];
    Style {
        outline: Rgba { r, g, b, a },
        outline_radius: 1.5,
        glow: Rgba { r, g, b, a: 0.7 },
        glow_radius: 10.,
    }
}

/// Appends integer digits without the general formatter in a fuel-limited callback.
fn integer(text: &mut String, number: i32) {
    if number < 0 {
        text.push('-');
    }
    let mut value = number.unsigned_abs();
    let mut digits = [0u8; 10];
    let mut start = digits.len();
    loop {
        start -= 1;
        digits[start] = b'0' + (value % 10) as u8;
        value /= 10;
        if value == 0 {
            break;
        }
    }
    for digit in &digits[start..] {
        text.push(char::from(*digit));
    }
}

/// Milliblock label anchors retain precision without expensive floating point formatting.
fn coordinate(text: &mut String, value: f32) {
    let rounded = (f64::from(value) * 1000.).round() as i64;
    if rounded < 0 {
        text.push('-');
    }
    let absolute = rounded.unsigned_abs();
    integer(text, (absolute / 1000) as i32);
    text.push('.');
    for divisor in [100, 10, 1] {
        text.push(char::from(b'0' + ((absolute % 1000 / divisor) % 10) as u8));
    }
}

/// Escapes untrusted server text directly into the HUD buffer, retaining UTF-8 slices.
fn quoted(output: &mut String, value: &str) {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    output.push('"');
    let mut start = 0;
    for (index, byte) in value.bytes().enumerate() {
        if byte == b'"' || byte == b'\\' || byte < 0x20 {
            output.push_str(&value[start..index]);
            output.push('\\');
            if byte < 0x20 {
                output.push_str("u00");
                output.push(char::from(HEX[usize::from(byte / 16)]));
                output.push(char::from(HEX[usize::from(byte % 16)]));
            } else {
                output.push(char::from(byte));
            }
            start = index + 1;
        }
    }
    output.push_str(&value[start..]);
    output.push('"');
}

#[cfg(test)]
mod tests;
