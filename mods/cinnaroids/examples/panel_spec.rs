//! Print the real guest panel specification for native UI previews.

use cinnaroids_mod::state::{AimMode, Preferences, State};

fn main() -> Result<(), serde_json::Error> {
    let mut state = State::new(Preferences::default());
    if std::env::args().any(|arg| arg == "--preview") {
        state.preferences.aim_strength = 100;
        state.preferences.aim_mode = AimMode::Continuous;
        state.modules.clicker = true;
        state.modules.aim = true;
        state.modules.reach = true;
    }
    println!("{}", state.panel_json()?);
    Ok(())
}
