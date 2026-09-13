import { describe, expect, it } from 'vitest'
import * as THREE from 'three'
import { CSMShadowNode } from 'three/addons/csm/CSMShadowNode.js'
import { computeSunLightSnapshot } from '../../utils/celestialSimulation'
import { setupCsmShadow } from './renderer-quality'
import {
  createSceneLightingController,
  type SceneLightingUpdateParams,
} from './scene-lighting'

function createLighting() {
  const light = new THREE.DirectionalLight()
  const date = { year: 217, month: 4, day: 1 }
  const params: SceneLightingUpdateParams = {
    currentPlayerPosition: new THREE.Vector3(),
    localCalendarDate: date,
    ambientLight: new THREE.AmbientLight(),
    directionalLight: light,
    directionalShadowsEnabled: true,
    scene: new THREE.Scene(),
    sunLightSnapshot: computeSunLightSnapshot(12, date),
    eclipseFactor: 0,
    cloudFactor: 0,
    rainIntensity: 0,
  }
  return { controller: createSceneLightingController(), light, params }
}

function initializeCascades(light: THREE.DirectionalLight) {
  setupCsmShadow(light)
  const csm = light.shadow.shadowNode as CSMShadowNode
  csm.setup({
    camera: new THREE.PerspectiveCamera(60, 1, 0.1, 500),
    renderer: {
      coordinateSystem: THREE.WebGPUCoordinateSystem,
      reversedDepthBuffer: false,
    },
  } as unknown as Parameters<CSMShadowNode['setup']>[0])
  expect(csm.lights).toHaveLength(2)
  return csm
}

describe('rain shadows', () => {
  it('fades every cascade in light rain and restores clear-weather shadows', () => {
    const { controller, light, params } = createLighting()
    const csm = initializeCascades(light)
    controller.update(params)
    const clearLightIntensity = light.intensity
    expect(light.shadow.intensity).toBe(1)

    let previous = light.shadow.intensity
    for (const rain of [0.01, 0.05, 0.1, 0.2]) {
      params.rainIntensity = rain
      controller.update(params)
      expect(light.shadow.intensity).toBeLessThan(previous)
      expect(light.castShadow).toBe(true)
      expect(light.intensity).toBe(clearLightIntensity)
      for (const cascade of csm.lights) {
        expect(cascade.shadow?.intensity).toBe(light.shadow.intensity)
      }
      previous = light.shadow.intensity
    }

    expect(light.shadow.intensity).toBeGreaterThan(0)
    expect(light.shadow.intensity).toBeLessThanOrEqual(0.05)
    params.rainIntensity = 1
    controller.update(params)
    expect(light.shadow.intensity).toBe(previous)

    params.rainIntensity = 0
    controller.update(params)
    expect(light.shadow.intensity).toBe(1)
    for (const cascade of csm.lights) {
      expect(cascade.shadow?.intensity).toBe(1)
    }
  })

  it('starts with faint shadows when cascades initialize during rain', () => {
    const { controller, light, params } = createLighting()
    controller.update({ ...params, rainIntensity: 0.2 })
    const csm = initializeCascades(light)
    for (const cascade of csm.lights) {
      expect(cascade.shadow?.intensity).toBeLessThanOrEqual(0.05)
    }
  })

  it.each(['night', 'underground', 'disabled'] as const)(
    'preserves shadow suppression when %s and restores daytime shadows',
    (mode) => {
      const { controller, light, params } = createLighting()
      controller.update({ ...params, rainIntensity: 0.2 })
      controller.update({
        ...params,
        underground: mode === 'underground',
        directionalShadowsEnabled: mode !== 'disabled',
        sunLightSnapshot: computeSunLightSnapshot(
          mode === 'night' ? 0 : 12,
          params.localCalendarDate
        ),
      })
      expect(light.castShadow).toBe(false)
      controller.update(params)
      expect(light.castShadow).toBe(true)
      expect(light.shadow.intensity).toBe(1)
    }
  )
})
