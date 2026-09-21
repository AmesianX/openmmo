import * as THREE from 'three'
import type { FishingStance } from '../stores/fishingStore'
import { findBoneByName } from './characterAnimationUtils'
import { RiderMotion, type HandTarget } from './riderMotion'

export class FishingReel {
  private readonly motion: RiderMotion
  private readonly hands: THREE.Bone[]
  private readonly rotor: THREE.Object3D | undefined
  private readonly handle: THREE.Object3D | undefined
  private readonly grip: THREE.Object3D | undefined
  private readonly tip: THREE.Object3D | undefined
  private readonly axis = new THREE.Vector3()
  private readonly rotorRotation = new THREE.Quaternion()
  private readonly targets: HandTarget[] = [
    { position: new THREE.Vector3() },
    {
      position: new THREE.Vector3(),
      gripOffset: new THREE.Vector3(),
      pole: new THREE.Vector3(-0.35, -0.7, -1),
    },
  ]
  private readonly fingers: (THREE.Bone[] | null)[]
  private readonly propPosition = new THREE.Vector3()
  private readonly propRotation = new THREE.Quaternion()
  private readonly propScale = new THREE.Vector3()
  private readonly propWorld = new THREE.Matrix4()
  private readonly localMatrix = new THREE.Matrix4()
  private readonly palm = new THREE.Vector3()
  private readonly drawBack = new THREE.Vector3()
  private readonly liftAxis = new THREE.Vector3()
  private readonly rotation = new THREE.Quaternion()
  private weight = 0
  private speed = 0
  private angle = 0
  private applied = false

  constructor(
    root: THREE.Object3D,
    private readonly prop: THREE.Object3D
  ) {
    this.motion = new RiderMotion(root)
    this.hands = ['Left', 'Right'].flatMap((side) => {
      const hand = findBoneByName(root, `${side}Hand`)
      return hand ? [hand] : []
    })
    this.fingers = this.hands.map((hand) => {
      const index = findBoneByName(root, `${hand.name}Index1`)
      const thumb = findBoneByName(root, `${hand.name}Thumb3`)
      return index && thumb ? [index, thumb] : null
    })
    this.rotor = prop.getObjectByName('reel_rotor')
    this.handle = prop.getObjectByName('reel_handle')
    this.grip = prop.getObjectByName('rod_grip')
    this.tip = prop.getObjectByName('rod_tip')
    const axis = prop.getObjectByName('reel_axis')
    if (axis) this.axis.copy(axis.position).normalize()
    if (this.rotor) this.rotorRotation.copy(this.rotor.quaternion)
  }

  restore() {
    if (!this.applied) return
    this.motion.restore()
    this.prop.position.copy(this.propPosition)
    this.prop.quaternion.copy(this.propRotation)
    this.prop.scale.copy(this.propScale)
    this.prop.updateWorldMatrix(true, true)
    this.applied = false
  }

  update(delta: number, stance: FishingStance | null, fishingIdle: boolean) {
    this.restore()
    if (
      !this.rotor ||
      !this.handle ||
      !this.grip ||
      !this.tip ||
      this.hands.length !== 2 ||
      this.axis.lengthSq() === 0 ||
      !this.prop.parent
    )
      return
    if (!fishingIdle) {
      this.weight = this.speed = this.angle = 0
      this.rotor.quaternion.copy(this.rotorRotation)
      return
    }
    const dt = Math.min(Math.max(delta, 0), 0.1)
    const blend = 1 - Math.exp(-dt * 12)
    this.weight = THREE.MathUtils.lerp(
      this.weight,
      stance === null ? 0 : 1,
      blend
    )
    const targetSpeed =
      stance === 'reel' ? -5.5 : stance === 'giveline' ? 3.8 : 0
    this.speed = THREE.MathUtils.lerp(this.speed, targetSpeed, blend)
    this.angle = (this.angle + this.speed * dt * this.weight) % (Math.PI * 2)
    this.rotor.quaternion
      .copy(this.rotorRotation)
      .multiply(this.rotation.setFromAxisAngle(this.axis, this.angle))
    if (this.weight < 0.001) return

    this.propPosition.copy(this.prop.position)
    this.propRotation.copy(this.prop.quaternion)
    this.propScale.copy(this.prop.scale)
    this.prop.updateWorldMatrix(true, true)
    this.propWorld.copy(this.prop.matrixWorld)
    // Lift the rod into a reachable two-hand stance.
    this.tip.getWorldPosition(this.drawBack)
    this.grip.getWorldPosition(this.palm)
    this.drawBack.sub(this.palm)
    this.liftAxis.set(-this.drawBack.z, 0, this.drawBack.x).normalize()
    this.rotation.setFromAxisAngle(this.liftAxis, 0.55 * this.weight)
    this.localMatrix.makeRotationFromQuaternion(this.rotation)
    this.liftAxis.copy(this.palm).applyQuaternion(this.rotation)
    this.localMatrix.setPosition(this.palm.sub(this.liftAxis))
    this.propWorld.premultiply(this.localMatrix)
    this.drawBack.multiplyScalar(-0.04 * this.weight)
    this.propWorld.elements[12] += this.drawBack.x
    this.propWorld.elements[14] += this.drawBack.z
    this.placeProp()
    for (let i = 0; i < this.hands.length; i++) {
      const hand = this.hands[i]
      const anchor = i === 0 ? this.grip : this.handle
      const fingers = this.fingers[i]
      if (fingers) {
        fingers[0].getWorldPosition(this.palm)
        fingers[1].getWorldPosition(this.targets[i].position)
        this.palm.add(this.targets[i].position).multiplyScalar(0.5)
      } else {
        hand.localToWorld(this.palm.set(0, 0.075, 0))
      }
      const target = this.targets[i]
      if (target.gripOffset) {
        hand.worldToLocal(target.gripOffset.copy(this.palm))
        anchor.getWorldPosition(target.position)
      } else {
        hand.getWorldPosition(target.position)
        this.palm.sub(target.position)
        anchor.getWorldPosition(target.position)
        target.position.sub(this.palm)
      }
    }
    this.motion.applyHandTargets(this.targets, this.weight)
    // The crank hand must not carry the rod around its circular path.
    this.placeProp()
    this.applied = true
  }

  private placeProp() {
    if (!this.prop.parent) return
    this.localMatrix
      .copy(this.prop.parent.matrixWorld)
      .invert()
      .multiply(this.propWorld)
    this.localMatrix.decompose(
      this.prop.position,
      this.prop.quaternion,
      this.prop.scale
    )
    this.prop.updateWorldMatrix(true, true)
  }
}
