//! What a player is riding, and the per-kind tuning that follows from it
//! (design: `doc/MOUNTS.md`). Shared so the server (authority), the web
//! client (prediction and rendering) and the agent-client (move timing) all
//! branch on the same table.

use serde::{Deserialize, Serialize};

const HORSE_MOVE_MULT: f32 = 3.0;
const HORSE_TURN_RADIUS: f32 = 0.65;
/// Comfortably better than wading (0.83x soaked) and under a sprinting
/// runner (1.5x) — a wooden boat that outran one read as a motorboat.
const ROWBOAT_MOVE_MULT: f32 = 1.25;
/// A hull turns wider than a horse.
const ROWBOAT_TURN_RADIUS: f32 = 1.2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MountKind {
    Horse,
    Rowboat,
}

impl MountKind {
    /// The `snake_case` wire spelling back to a kind — what the client holds
    /// in `Player.mount`, so it can ask the table through wasm by name.
    pub fn from_wire(kind: &str) -> Option<Self> {
        match kind {
            "horse" => Some(Self::Horse),
            "rowboat" => Some(Self::Rowboat),
            _ => None,
        }
    }

    /// The carried item that boards and leaves it. Held every tick: losing
    /// it ends the ride.
    pub fn item_id(self) -> &'static str {
        match self {
            Self::Horse => "horse_reins",
            Self::Rowboat => "rowboat",
        }
    }

    /// Multiplier on walk and sprint speed alike.
    pub fn speed_mult(self) -> f32 {
        match self {
            Self::Horse => HORSE_MOVE_MULT,
            Self::Rowboat => ROWBOAT_MOVE_MULT,
        }
    }

    /// Radius of the arc a turn travels, rather than pivoting in place.
    pub fn turn_radius(self) -> f32 {
        match self {
            Self::Horse => HORSE_TURN_RADIUS,
            Self::Rowboat => ROWBOAT_TURN_RADIUS,
        }
    }

    /// Whether being drawn into combat throws the rider off.
    pub fn dismounts_in_combat(self) -> bool {
        match self {
            Self::Horse => true,
            // Nowhere to stand: throwing the rider out here would drop them
            // into open water.
            Self::Rowboat => false,
        }
    }

    /// Whether it rides on the water rather than through it. The rider's Y
    /// becomes the surface, and the soaking a wader would take never lands.
    pub fn floats(self) -> bool {
        match self {
            Self::Horse => false,
            Self::Rowboat => true,
        }
    }

    /// Refusal shown when the spot underfoot is wrong for this mount.
    pub fn cannot_mount_here_message(self) -> &'static str {
        match self {
            Self::Horse => "Mount on outdoor ground while alive and out of combat.",
            Self::Rowboat => "Launch the boat while standing in water, outdoors and alive.",
        }
    }

    /// Water depth the mount stays between, in metres. Land reads as a
    /// negative depth (the water surface collapses below the bed), so a
    /// land mount's band simply has no floor.
    pub fn water_depth_band(self) -> (f32, f32) {
        match self {
            Self::Horse => (f32::NEG_INFINITY, 0.6),
            // Anything a rod can reach floats it, so "where can I fish?" and
            // "where can I row?" stay one rule the player learns once.
            Self::Rowboat => (crate::fishing::MIN_FISHABLE_DEPTH_M, f32::INFINITY),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::MountKind;

    /// `from_wire` is a hand-written mirror of the serde spelling; a variant
    /// added to one and not the other would make the client's wasm lookups
    /// answer "on foot" for a real mount.
    #[test]
    fn every_kind_round_trips_through_its_wire_name() {
        for kind in [MountKind::Horse, MountKind::Rowboat] {
            let wire = serde_json::to_value(kind).unwrap();
            let name = wire.as_str().expect("kinds serialize as bare strings");
            assert_eq!(MountKind::from_wire(name), Some(kind), "{name}");
        }
        assert_eq!(MountKind::from_wire("griffin"), None);
    }
}
