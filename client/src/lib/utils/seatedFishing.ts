import * as THREE from 'three'

/** Bones the seated pose keeps while the upper body fishes. `Spine` is the
 *  boundary and stays with the legs: handing it to the cast would swing the
 *  waist out over hips that are still sitting down. */
const SEATED_BONES = new Set([
  'Hips',
  'Spine',
  'LeftUpLeg',
  'LeftLeg',
  'LeftFoot',
  'LeftToeBase',
  'LeftToe_End',
  'RightUpLeg',
  'RightLeg',
  'RightFoot',
  'RightToeBase',
  'RightToe_End',
])

/** Retargeted clips name tracks `.bones[Name].prop`; raw GLB clips use
 *  `Name.prop`. Both reach here, so read the bone out of either. */
const RETARGETED = /^\.bones\[(.+?)\]\.(.+)$/

function boneOf(trackName: string): string {
  const retargeted = RETARGETED.exec(trackName)
  if (retargeted) return retargeted[1]
  const dot = trackName.lastIndexOf('.')
  return dot === -1 ? trackName : trackName.slice(0, dot)
}

/** The track's first keyframe, held for the whole clip. */
function frozen(track: THREE.KeyframeTrack): THREE.KeyframeTrack {
  const held = track.clone()
  held.times = new Float32Array([0])
  held.values = track.values.slice(0, track.getValueSize())
  return held
}

const merged = new WeakMap<
  THREE.AnimationClip,
  WeakMap<THREE.AnimationClip, THREE.AnimationClip>
>()

/**
 * Fish from a chair: the legs hold the seated pose while the spine and arms
 * play the fishing clip. Cheaper and steadier than authoring a second set of
 * clips — there is nothing to retarget, and any future fishing clip gets a
 * seated variant for free.
 *
 * The seated half is frozen at its first frame rather than played: a sitting
 * angler's legs do not need their own loop, and holding them keeps the two
 * clips from drifting out of phase over a long fight.
 */
export function seatedFishingClip(
  fishing: THREE.AnimationClip,
  seated: THREE.AnimationClip
): THREE.AnimationClip {
  let bySeat = merged.get(fishing)
  if (!bySeat) {
    bySeat = new WeakMap()
    merged.set(fishing, bySeat)
  }
  const cached = bySeat.get(seated)
  if (cached) return cached

  const tracks: THREE.KeyframeTrack[] = fishing.tracks.filter(
    (track) => !SEATED_BONES.has(boneOf(track.name))
  )
  for (const track of seated.tracks) {
    if (SEATED_BONES.has(boneOf(track.name))) tracks.push(frozen(track))
  }

  const clip = new THREE.AnimationClip(
    fishing.name,
    fishing.duration,
    tracks,
    fishing.blendMode
  )
  bySeat.set(seated, clip)
  return clip
}
