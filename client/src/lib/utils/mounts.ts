import type { MountKind } from '../network/networkTypes'
import {
  mount_floats,
  mount_speed_mult,
  mount_turn_radius,
} from '../wasm/onlinerpg_shared'

/** The TS half of `Player::is_mounted()` — one place that knows the field
 *  is `MountKind | null | undefined`, so call sites ask a yes/no question.
 *  A type guard, so a passing check also narrows the player to non-null and
 *  its mount to a kind, the way `?.mount == null` used to narrow the player. */
export function isMounted<P extends { mount?: MountKind | null }>(
  player: P | null | undefined
): player is P & { mount: MountKind } {
  return player?.mount != null
}

// The numbers live in shared/src/mount.rs and arrive through wasm, so the
// prediction here cannot drift from what the server simulates.

export function mountSpeedMult(mount?: MountKind | null): number {
  return mount ? mount_speed_mult(mount) : 1
}

export function mountTurnRadius(mount: MountKind): number {
  return mount_turn_radius(mount)
}

export function mountFloats(mount?: MountKind | null): boolean {
  return mount ? mount_floats(mount) : false
}
