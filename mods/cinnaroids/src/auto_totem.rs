//! Offhand replenishment policy using known, session-bound equipment observations.

use mod_api::bindings::cinnabar::extension::player_state::{Equipment, Revisions, Slot};

/// Exact item identifier matched against observed equipment.
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
    pub fn select(&mut self, equipment: &Equipment) -> Option<u8> {
        if holds_totem(&equipment.offhand) != Some(false) {
            return None;
        }
        let slot = equipment
            .inventory
            .iter()
            .take(mod_api::PLAYER_STATE_INVENTORY_SLOTS)
            .position(|slot| holds_totem(slot) == Some(true))?;
        self.remaining = RETRY_SECONDS;
        self.retry = true;
        u8::try_from(slot).ok()
    }
}

/// Whether a cell holds a totem: none while the cell or its item identifier is unknown,
/// false for an empty cell or another known item.
fn holds_totem(slot: &Slot) -> Option<bool> {
    if !slot.known {
        return None;
    }
    match &slot.item {
        None => Some(false),
        Some(item) => item.identifier.as_deref().map(|id| id == TOTEM),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mod_api::bindings::cinnabar::extension::player_state::Item;

    fn known(identifier: Option<&str>) -> Slot {
        Slot {
            known: true,
            item: identifier.map(|identifier| Item {
                identifier: Some(identifier.into()),
                network_id: 1,
                metadata: 0,
                count: 1,
                block: false,
                damage: None,
                max_durability: None,
            }),
        }
    }

    /// Equipment with one totem in inventory cell 20 and a known empty offhand.
    fn equipment() -> Equipment {
        let mut inventory = vec![known(None); mod_api::PLAYER_STATE_INVENTORY_SLOTS];
        inventory[20] = known(Some(TOTEM));
        Equipment {
            session: 1,
            dimension: 0,
            selected_slot: Some(0),
            inventory,
            armor: Vec::new(),
            offhand: known(None),
        }
    }

    fn revisions() -> Revisions {
        Revisions {
            session: 1,
            dimension: 0,
            equipment: 1,
            effects: 1,
        }
    }

    #[test]
    fn fills_and_replenishes_without_disturbing_an_equipped_totem() {
        let mut policy = AutoTotem::default();
        let mut e = equipment();
        assert_eq!(policy.select(&e), Some(20));
        e.offhand = known(Some(TOTEM));
        assert_eq!(policy.select(&e), None);
        e.offhand = known(Some("minecraft:shield"));
        assert_eq!(policy.select(&e), Some(20));
    }

    #[test]
    fn prefers_the_first_hotbar_match() {
        let mut e = equipment();
        e.inventory[3] = known(Some(TOTEM));
        assert_eq!(AutoTotem::default().select(&e), Some(3));
    }

    #[test]
    fn unknown_offhand_and_missing_inventory_matches_are_left_alone() {
        let mut policy = AutoTotem::default();
        let mut e = equipment();
        e.offhand = Slot {
            known: false,
            item: None,
        };
        assert_eq!(policy.select(&e), None);
        e.offhand = known(Some("minecraft:shield"));
        e.offhand.item.as_mut().unwrap().identifier = None;
        assert_eq!(policy.select(&e), None);
        e.offhand = known(None);
        e.inventory[20].known = false;
        assert_eq!(policy.select(&e), None);
        e.inventory[20] = known(Some("minecraft:shield"));
        e.inventory.push(known(Some(TOTEM)));
        assert_eq!(policy.select(&e), None);
    }

    #[test]
    fn retries_are_bounded_and_context_changes_clear_old_work() {
        let mut p = AutoTotem::default();
        let mut r = revisions();
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
