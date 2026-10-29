//! Module requirements, checked against explicit host grants rather than world availability.

use crate::state::Modules;
use mod_api::bindings::cinnabar::extension::permissions::Permission;

/// One module's required capabilities; each inner group accepts any one grant.
pub struct Requirement {
    pub id: &'static str,
    pub groups: &'static [&'static [&'static str]],
    pub flag: fn(&mut Modules) -> &mut bool,
}

/// Module order matches the panel's module rows. Optional presentation is not required.
pub const REQUIREMENTS: [Requirement; 9] = [
    Requirement {
        id: "clicker",
        groups: &[&["controls"], &["interaction"], &["players", "terrain"]],
        flag: |m| &mut m.clicker,
    },
    Requirement {
        id: "aim",
        groups: &[&["controls"], &["players"], &["camera"]],
        flag: |m| &mut m.aim,
    },
    Requirement {
        id: "reach",
        groups: &[&["controls"], &["interaction"], &["players", "terrain"]],
        flag: |m| &mut m.reach,
    },
    Requirement {
        id: "jump_reset",
        groups: &[&["controls"], &["movement"]],
        flag: |m| &mut m.jump_reset,
    },
    Requirement {
        id: "fake_lag",
        groups: &[&["controls"], &["packet_delay"]],
        flag: |m| &mut m.fake_lag,
    },
    Requirement {
        id: "netherite",
        groups: &[&["controls"], &["terrain"], &["world_draw"]],
        flag: |m| &mut m.netherite,
    },
    Requirement {
        id: "fullbright",
        groups: &[&["controls"], &["fullbright"]],
        flag: |m| &mut m.fullbright,
    },
    Requirement {
        id: "player_esp",
        groups: &[
            &["controls"],
            &["players"],
            &["player_state"],
            &["hud"],
            &["world_draw"],
            &["entity_observe"],
            &["entity_highlight"],
            &["render_through_walls"],
        ],
        flag: |m| &mut m.player_esp,
    },
    Requirement {
        id: "auto_totem",
        groups: &[&["controls"], &["player_state"], &["inventory"]],
        flag: |m| &mut m.auto_totem,
    },
];

/// Missing-permission explanations; an empty string means the module is available.
#[derive(Clone, Debug)]
pub struct Access(pub [String; 9]);

impl Default for Access {
    fn default() -> Self {
        Self(std::array::from_fn(|_| "Permissions unavailable".into()))
    }
}

impl Access {
    /// Resolves all requirements from the host's per-instance permission declaration.
    pub fn read(permissions: &[Permission]) -> Self {
        Self(std::array::from_fn(|index| {
            let missing: Vec<_> = REQUIREMENTS[index]
                .groups
                .iter()
                .filter_map(|group| {
                    if group
                        .iter()
                        .any(|name| permissions.iter().any(|p| p.name == *name && p.granted))
                    {
                        return None;
                    }
                    Some(
                        group
                            .iter()
                            .map(|name| {
                                permissions
                                    .iter()
                                    .find(|p| p.name == *name)
                                    .map_or(*name, |p| p.label.as_str())
                            })
                            .collect::<Vec<_>>()
                            .join(" or "),
                    )
                })
                .collect();
            if missing.is_empty() {
                String::new()
            } else {
                format!("Requires: {}", missing.join(", "))
            }
        }))
    }

    /// Removes enabled flags whose required authority is absent.
    pub fn enforce(&self, modules: &mut Modules) -> bool {
        let mut changed = false;
        for (requirement, reason) in REQUIREMENTS.iter().zip(&self.0) {
            let enabled = (requirement.flag)(modules);
            if !reason.is_empty() && *enabled {
                *enabled = false;
                changed = true;
            }
        }
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{Controls, Event, Preferences, State};

    /// Builds actual SDK permission records for every capability the module policy uses.
    fn grants() -> Vec<Permission> {
        let mut names = std::collections::BTreeSet::new();
        for requirement in &REQUIREMENTS {
            names.extend(
                requirement
                    .groups
                    .iter()
                    .flat_map(|group| group.iter().copied()),
            );
        }
        names
            .into_iter()
            .map(|name| Permission {
                name: name.into(),
                label: name.into(),
                granted: true,
            })
            .collect()
    }

    #[test]
    fn every_required_permission_blocks_its_module_and_restoring_it_allows_enabling() {
        for (index, requirement) in REQUIREMENTS.iter().enumerate() {
            for group in requirement.groups {
                let mut grants = grants();
                for p in &mut grants {
                    if group.contains(&p.name.as_str()) {
                        p.granted = false;
                    }
                }
                let mut state = State::new(Preferences::default());
                state.set_permissions(&grants);
                let events = [Event {
                    id: requirement.id.into(),
                    value: 1.,
                }];
                state.controls(Controls {
                    focused: true,
                    panel_open: true,
                    keys: &[],
                    held: &[],
                    seconds: 0.,
                    events: &events,
                });
                assert!(!*(requirement.flag)(&mut state.modules));
                assert!(!state.access.0[index].is_empty());
                for p in &mut grants {
                    p.granted = true;
                }
                state.set_permissions(&grants);
                state.controls(Controls {
                    focused: true,
                    panel_open: true,
                    keys: &[],
                    held: &[],
                    seconds: 0.,
                    events: &events,
                });
                assert!(*(requirement.flag)(&mut state.modules));
                state.set_permissions(&[]);
                assert!(!*(requirement.flag)(&mut state.modules));
            }
        }
    }

    #[test]
    fn denied_modules_cannot_be_enabled_by_shortcuts() {
        let mut state = State::new(Preferences {
            clicker_key: "KeyC".into(),
            aim_key: "KeyA".into(),
            reach_key: "KeyR".into(),
            jump_reset_key: "KeyJ".into(),
            ..Default::default()
        });
        state.set_permissions(&[]);
        let keys = ["KeyC", "KeyA", "KeyR", "KeyJ"].map(String::from);
        state.controls(Controls {
            focused: true,
            panel_open: false,
            keys: &keys,
            held: &[],
            seconds: 0.1,
            events: &[],
        });
        assert!(state.enabled_flags().into_iter().all(|enabled| !enabled));
    }

    #[test]
    fn local_pose_accepts_either_grant_and_optional_drawing_does_not_block_fake_lag() {
        let mut grants = grants();
        for p in &mut grants {
            if ["players", "world_draw"].contains(&p.name.as_str()) {
                p.granted = false;
            }
        }
        let access = Access::read(&grants);
        for index in [0, 2, 4, 8] {
            assert!(access.0[index].is_empty());
        }
    }
}
