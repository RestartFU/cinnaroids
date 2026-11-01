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
    if std::env::args().any(|arg| arg == "--nuker") {
        let permissions = cinnaroids_mod::permissions::REQUIREMENTS
            .iter()
            .flat_map(|r| r.groups.iter().flat_map(|g| g.iter().copied()))
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .map(
                |name| mod_api::bindings::cinnabar::extension::permissions::Permission {
                    name: name.into(),
                    label: name.into(),
                    granted: true,
                },
            )
            .collect::<Vec<_>>();
        state.set_permissions(&permissions);
        state.modules.nuker = true;
        state.preferences.nuker_key = "KeyN".into();
        state.expanded[10] = true;
    }
    println!("{}", state.panel_json()?);
    Ok(())
}
