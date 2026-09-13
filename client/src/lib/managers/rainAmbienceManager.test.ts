import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

vi.mock('./sfxManager', () => ({
  getSfxMultiplier: vi.fn(() => 1),
}))

import { stopRainAmbience, updateRainAmbience } from './rainAmbienceManager'
import { getSfxMultiplier } from './sfxManager'

interface FakeGain {
  gain: { value: number }
}

interface FakeContext {
  started: number
  gains: FakeGain[]
  pending: { url: string; resolve: (value: unknown) => void }[]
  release: (failedUrl?: string) => void
}

const contexts: FakeContext[] = []

function installFakeAudio() {
  class FakeAudioContext {
    state = 'running'
    destination = {}
    started = 0
    gains: FakeGain[] = []
    pending: FakeContext['pending'] = []
    release = (failedUrl?: string) => {
      for (const { url, resolve } of this.pending)
        resolve({
          ok: url !== failedUrl,
          status: url === failedUrl ? 404 : 200,
          arrayBuffer: async () => new ArrayBuffer(4),
        })
    }
    createGain() {
      const gain = { gain: { value: 0 }, connect() {} }
      this.gains.push(gain)
      return gain
    }
    createBufferSource() {
      return {
        buffer: null,
        loop: false,
        playbackRate: { value: 1 },
        connect() {},
        start: () => {
          this.started++
        },
      }
    }
    createBiquadFilter() {
      return { type: '', frequency: { value: 0 }, connect() {} }
    }
    decodeAudioData = async (data: ArrayBuffer) => data
    resume = async () => {}
    close = async () => {}
    constructor() {
      contexts.push(this)
    }
  }
  vi.stubGlobal('AudioContext', FakeAudioContext)
  vi.stubGlobal('fetch', (url: string) => {
    const ctx = contexts[contexts.length - 1]
    return new Promise((resolve) => ctx.pending.push({ url, resolve }))
  })
}

const flush = () => new Promise((resolve) => setTimeout(resolve, 0))

describe('rainAmbienceManager', () => {
  beforeEach(() => {
    contexts.length = 0
    vi.mocked(getSfxMultiplier).mockReturnValue(1)
    installFakeAudio()
  })
  afterEach(() => {
    stopRainAmbience()
    vi.unstubAllGlobals()
  })

  it('does not initialize audio in dry weather', () => {
    updateRainAmbience(0, false, 1)
    expect(contexts).toHaveLength(0)
  })

  it('starts both loops on the context the load began for', async () => {
    updateRainAmbience(1, false, 0.016)
    expect(contexts).toHaveLength(1)
    contexts[0].release()
    await flush()
    expect(contexts[0].started).toBe(2)
    expect(contexts[0].pending.map(({ url }) => url)).toEqual([
      '/sounds/rain-drops-loop.ogg',
      '/sounds/rain-loop.ogg',
      '/sounds/thunder-distant.ogg',
    ])
  })

  it.each([0.05, 0.2, 0.45])('plays only droplets at intensity %s', (rain) => {
    updateRainAmbience(rain, false, 1)
    const [drops, heavy] = contexts[0].gains
    expect(drops.gain.value).toBeCloseTo(rain * 0.5)
    expect(heavy.gain.value).toBe(0)
  })

  it('blends toward the original recording as rain strengthens', () => {
    updateRainAmbience(0.7, false, 1)
    const [drops, heavy] = contexts[0].gains
    expect(drops.gain.value).toBeGreaterThan(0)
    expect(heavy.gain.value).toBeGreaterThan(0)
    expect(drops.gain.value + heavy.gain.value).toBeCloseTo(0.7 * 0.5)

    updateRainAmbience(1, false, 1)
    expect(drops.gain.value).toBe(0)
    expect(heavy.gain.value).toBeCloseTo(0.5)
  })

  it('fades between layers and back to silence', () => {
    updateRainAmbience(1, false, 1)
    updateRainAmbience(0.2, false, 0.1)
    const [drops, heavy] = contexts[0].gains
    expect(drops.gain.value).toBeGreaterThan(0)
    expect(drops.gain.value).toBeLessThan(0.1)
    expect(heavy.gain.value).toBeGreaterThan(0)
    expect(heavy.gain.value).toBeLessThan(0.5)

    for (let frame = 0; frame < 600; frame++)
      updateRainAmbience(0, false, 1 / 60)
    expect(drops.gain.value).toBeLessThan(0.00001)
    expect(heavy.gain.value).toBeLessThan(0.00001)
  })

  it('attenuates both layers indoors without changing their balance', () => {
    updateRainAmbience(0.7, false, 1)
    const outdoors = contexts[0].gains.map(({ gain }) => gain.value)
    updateRainAmbience(0.7, true, 1)
    contexts[0].gains.forEach(({ gain }, i) => {
      expect(gain.value).toBeCloseTo(outdoors[i] * 0.35)
    })
  })

  it('applies SFX volume and immediate mute to both layers', () => {
    updateRainAmbience(0.7, false, 1)
    const fullVolume = contexts[0].gains.map(({ gain }) => gain.value)
    vi.mocked(getSfxMultiplier).mockReturnValue(0.4)
    updateRainAmbience(0.7, false, 0.016)
    contexts[0].gains.forEach(({ gain }, i) => {
      expect(gain.value).toBeCloseTo(fullVolume[i] * 0.4)
    })

    vi.mocked(getSfxMultiplier).mockReturnValue(0)
    updateRainAmbience(0.7, false, 0.016)
    expect(contexts[0].gains.map(({ gain }) => gain.value)).toEqual([0, 0])
  })

  it.each([
    ['/sounds/rain-drops-loop.ogg', 1],
    ['/sounds/rain-loop.ogg', 1],
    ['/sounds/thunder-distant.ogg', 2],
  ])('keeps other layers playing when %s fails', async (url, started) => {
    updateRainAmbience(0.7, false, 0.016)
    contexts[0].release(url)
    await flush()
    expect(contexts[0].started).toBe(started)
  })

  it('drops a load whose context was stopped before the files arrived', async () => {
    updateRainAmbience(1, false, 0.016)
    stopRainAmbience()
    updateRainAmbience(1, false, 0.016)
    expect(contexts).toHaveLength(2)

    contexts[0].release()
    contexts[1].release()
    await flush()

    expect(contexts[0].started).toBe(0)
    expect(contexts[1].started).toBe(2)
  })

  it('clears the previous storm mix on restart', () => {
    updateRainAmbience(1, false, 1)
    stopRainAmbience()
    updateRainAmbience(0.2, false, 0.016)
    const [drops, heavy] = contexts[1].gains
    expect(drops.gain.value).toBeGreaterThan(0)
    expect(heavy.gain.value).toBe(0)
  })
})
