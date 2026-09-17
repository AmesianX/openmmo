import * as THREE from 'three'
import { clone } from 'three/examples/jsm/utils/SkeletonUtils.js'
import type { GLTF } from 'three/examples/jsm/loaders/GLTFLoader.js'

export const ROWBOAT_MODEL_PATH = '/models/mounts/rowboat.glb'

/** Metres of swell at the gunwale, and how long one cycle takes. */
const BOB_HEIGHT = 0.035
const BOB_SECONDS = 3.4
const ROLL_RADIANS = 0.035
const OAR_SWEEP = 0.5
const OAR_STROKE_SECONDS = 1.9

/** The rowboat under its rider. Unlike the horse this is rigid — the hull
 *  carries no skeleton and no clips, so the swell and the oar stroke are
 *  driven here rather than retargeted onto the character. */
export class BoatMount {
  readonly root: THREE.Object3D
  readonly seat: THREE.Object3D
  private readonly oars: THREE.Object3D[]
  private readonly oarRest: number[]
  private elapsed = 0
  private stroke = 0

  constructor(gltf: GLTF) {
    this.root = clone(gltf.scene)
    this.seat = this.root.getObjectByName('RideSeat') ?? this.root
    this.oars = ['OarPort', 'OarStarboard']
      .map((name) => this.root.getObjectByName(name))
      .filter((node): node is THREE.Object3D => node !== undefined)
    this.oarRest = this.oars.map((oar) => oar.rotation.z)
    this.root.traverse((node) => {
      if (node instanceof THREE.Mesh) {
        node.castShadow = true
        node.receiveShadow = true
      }
    })
  }

  update(dt: number, speed: number) {
    this.elapsed += dt
    const swell = (this.elapsed / BOB_SECONDS) * Math.PI * 2
    this.root.position.y = Math.sin(swell) * BOB_HEIGHT
    this.root.rotation.z = Math.cos(swell * 0.7) * ROLL_RADIANS

    // Oars sweep only while making way; at rest they stay shipped.
    if (speed > 0.05) {
      this.stroke += (dt / OAR_STROKE_SECONDS) * Math.PI * 2
    }
    const sweep = speed > 0.05 ? Math.sin(this.stroke) * OAR_SWEEP : 0
    this.oars.forEach((oar, i) => {
      oar.rotation.z = this.oarRest[i] + (i === 0 ? sweep : -sweep)
    })
  }

  /** Nothing to release: the clone shares its geometry with the cached GLB,
   *  which the next boarding reuses. Mirrors HorseMount, which also leaves
   *  the shared buffers alone. */
  dispose() {}
}
