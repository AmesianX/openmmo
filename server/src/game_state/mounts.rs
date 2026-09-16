use onlinerpg_shared::mount::MountKind;

use super::GameState;
use crate::types::{Player, PlayerId, ServerMessage};

impl GameState {
    /// Whether the player can still work the mount under them: aboard, alive,
    /// and not in a fight that throws them. Combat throws a horse's rider but
    /// not a boat's, which has nowhere to put them.
    ///
    /// The state half of `can_ride_here`'s question, kept apart from it
    /// because this one is called from the movement tick, where the water
    /// sample that decides the *place* must never run. A bad place ends the
    /// ride on the next `validate_mounts`; a bad state only drops the input.
    pub(super) fn can_steer_mount(player: &Player) -> bool {
        let Some(kind) = player.mount else {
            return false;
        };
        if player.health == 0 {
            return false;
        }
        if kind.dismounts_in_combat() && Self::in_combat(player) {
            return false;
        }
        true
    }

    pub(super) async fn can_ride_here(&self, player: &Player, kind: MountKind) -> bool {
        if player.health == 0
            || player.floor_level != 0
            || player.object_type.is_some()
            // The rider is not on it yet, so ask the kind being boarded.
            || (kind.dismounts_in_combat() && Self::in_combat(player))
        {
            return false;
        }
        let inside = self.passability_read().values().any(|entry| {
            entry.is_ground
                && player.position.x >= entry.min_x
                && player.position.x <= entry.max_x
                && player.position.z >= entry.min_z
                && player.position.z <= entry.max_z
        });
        let (min_depth, max_depth) = kind.water_depth_band();
        !inside
            && self
                .water_depth_at(player.position.x, player.position.z)
                .await
                .is_some_and(|depth| depth >= min_depth && depth <= max_depth)
    }

    pub(super) async fn toggle_mount(&self, player_id: &PlayerId, kind: MountKind) {
        let Some(player) = self.players.read().await.get(player_id).cloned() else {
            return;
        };
        if player.is_mounted() {
            self.set_mount(player_id, None).await;
            return;
        }
        if !self.can_ride_here(&player, kind).await {
            self.send_system_message(player_id, kind.cannot_mount_here_message())
                .await;
            return;
        }
        if !self.holds_item(player_id, kind.item_id()).await {
            return;
        }
        self.set_mount(player_id, Some(kind)).await;
    }

    pub(super) async fn set_mount(&self, player_id: &PlayerId, mount: Option<MountKind>) {
        let Some((was, at, floor)) = self
            .players
            .read()
            .await
            .get(player_id)
            .map(|p| (p.mount, p.position, p.floor_level))
        else {
            return;
        };
        if was == mount {
            return;
        }
        // A boat lifts its rider to the surface the moment they board, and
        // sets them back on the bed when they leave — not on the next step.
        // Only outdoors: a rider whose dungeon floor ended the ride is below
        // the surface this would compute, and must not be dragged up to it.
        let lifted = if floor == 0 {
            Some(self.surface_ground_y(0, &at, at.y, mount).await)
        } else {
            None
        };
        let (position, rotation, moved) = {
            let mut players = self.players.write().await;
            let Some(player) = players.get_mut(player_id) else {
                return;
            };
            // Re-check under the write lock: two `use_item` calls racing here
            // would otherwise both pass the read above and toggle twice.
            if player.mount != was {
                return;
            }
            player.mount = mount;
            // The read above was dropped across an await, so the lift only
            // applies if the mover has not stepped since.
            let moved = match lifted {
                Some(y) if player.position.x == at.x && player.position.z == at.z => {
                    let changed = (y - player.position.y).abs() > 1e-3;
                    player.position.y = y;
                    changed
                }
                _ => false,
            };
            (player.position, player.rotation, moved)
        };
        if moved {
            self.publish_nearby(
                &position,
                floor,
                ServerMessage::PlayerMoved {
                    player_id: *player_id,
                    position,
                    rotation,
                    floor_level: floor,
                    sprinting: false,
                },
                None,
            )
            .await;
        }
        self.publish_nearby(
            &position,
            floor,
            ServerMessage::PlayerMountChanged {
                player_id: *player_id,
                mount,
            },
            None,
        )
        .await;
    }

    pub(super) async fn validate_mounts(&self) {
        let riders: Vec<_> = self
            .players
            .read()
            .await
            .values()
            .filter_map(|player| player.mount.map(|kind| (player.clone(), kind)))
            .collect();
        for (player, kind) in riders {
            if !self.can_ride_here(&player, kind).await
                || !self.holds_item(&player.id, kind.item_id()).await
            {
                self.set_mount(&player.id, None).await;
            }
        }
    }
}
