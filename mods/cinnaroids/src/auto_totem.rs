//! Offhand replenishment policy using known, session-bound equipment observations.

use mod_api::bindings::cinnabar::extension::player_state::{EquipmentMatches, Revisions};

/// Exact item identifier requested from the host equipment query.
pub const TOTEM: &str = "minecraft:totem_of_undying";
/// Rejected or delayed requests are retried at most four times per second.
const RETRY_SECONDS: f32 = 0.25;

/// Tracks equipment changes and bounds retries without retaining inventory copies.
#[derive(Default)]
pub struct AutoTotem {
    observed: Option<(u64, i32, u64)>,
    retry: bool,
    remaining: f32,
}

impl AutoTotem {
    /// Discards pending work when disabled or gameplay loses input ownership.
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Requests a full equipment read only on change or after a pending swap's retry delay.
    pub fn observe(&mut self, revision: Revisions, seconds: f32) -> bool {
        if !seconds.is_finite() || seconds < 0.0 {
            self.reset();
            return false;
        }
        let context = (revision.session, revision.dimension, revision.equipment);
        if self
            .observed
            .is_some_and(|old| (old.0, old.1) != (context.0, context.1))
        {
            self.reset();
        }
        self.remaining = (self.remaining - seconds).max(0.0);
        if self.remaining > 0.0 || (self.observed == Some(context) && !self.retry) {
            return false;
        }
        self.observed = Some(context);
        self.retry = false;
        true
    }

    /// Finds a known totem and starts a bounded retry window for the requested swap.
    pub fn select(&mut self, equipment: &EquipmentMatches) -> Option<u8> {
        if equipment.offhand != Some(false) {
            return None;
        }
        let slot = equipment
            .inventory
            .iter()
            .copied()
            .find(|slot| usize::from(*slot) < mod_api::PLAYER_STATE_INVENTORY_SLOTS)?;
        self.remaining = RETRY_SECONDS;
        self.retry = true;
        Some(slot)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Exact query response containing one inventory match and a known empty offhand.
    fn equipment() -> EquipmentMatches {
        EquipmentMatches {
            revisions: Revisions {
                session: 1,
                dimension: 0,
                equipment: 1,
                effects: 1,
            },
            inventory: vec![20],
            offhand: Some(false),
        }
    }

    #[test]
    fn fills_and_replenishes_without_disturbing_an_equipped_totem() {
        let mut policy = AutoTotem::default();
        let mut e = equipment();
        assert_eq!(policy.select(&e), Some(20));
        e.offhand = Some(true);
        assert_eq!(policy.select(&e), None);
        e.offhand = Some(false);
        assert_eq!(policy.select(&e), Some(20));
    }

    #[test]
    fn unknown_offhand_and_missing_inventory_matches_are_left_alone() {
        let mut policy = AutoTotem::default();
        let mut e = equipment();
        e.offhand = None;
        assert_eq!(policy.select(&e), None);
        e.offhand = Some(false);
        e.inventory.clear();
        assert_eq!(policy.select(&e), None);
        e.inventory = vec![36];
        assert_eq!(policy.select(&e), None);
    }

    #[test]
    fn retries_are_bounded_and_context_changes_clear_old_work() {
        let mut p = AutoTotem::default();
        let mut r = equipment().revisions;
        assert!(p.observe(r, 0.0));
        assert_eq!(p.select(&equipment()), Some(20));
        assert!(!p.observe(r, 0.1));
        assert!(p.observe(r, 0.2));
        assert!(!p.observe(r, 0.0));
        r.equipment += 1;
        assert!(p.observe(r, 0.0));
        p.select(&equipment());
        r.session += 1;
        assert!(p.observe(r, 0.0));
        p.reset();
        assert!(p.observe(r, 0.0));
        assert!(!p.observe(r, f32::NAN));
    }
}
