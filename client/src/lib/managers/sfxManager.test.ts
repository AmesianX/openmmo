import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import {
  cancelPendingFishingSounds,
  playDungeonDripSound,
  playFishingSound,
  sfxMuted,
  sfxVolume,
  stopDungeonDripSounds,
} from './sfxManager'

// sfxManager caches audio pools at module level, so FakeAudio instances
// survive across tests — count plays in a per-test map instead.
let plays = new Map<string, number>()
let playbacks: { audio: FakeAudio; volume: number; rate: number }[] = []

class FakeAudio {
  preload = ''
  volume = 1
  currentTime = 0
  playbackRate = 1
  paused = true
  constructor(public url: string) {}
  load() {}
  play() {
    this.paused = false
    playbacks.push({
      audio: this,
      volume: this.volume,
      rate: this.playbackRate,
    })
    plays.set(this.url, (plays.get(this.url) ?? 0) + 1)
    return Promise.resolve()
  }
  pause() {
    this.paused = true
  }
}

function playedCount(urlPart: string) {
  let sum = 0
  for (const [url, count] of plays) {
    if (url.includes(urlPart)) sum += count
  }
  return sum
}

describe('delayed fishing sounds', () => {
  beforeEach(() => {
    plays = new Map()
    vi.useFakeTimers()
    vi.stubGlobal('Audio', FakeAudio)
    vi.stubGlobal('window', {
      setTimeout: setTimeout.bind(globalThis),
      clearTimeout: clearTimeout.bind(globalThis),
    })
  })

  afterEach(() => {
    cancelPendingFishingSounds()
    vi.unstubAllGlobals()
    vi.useRealTimers()
  })

  it('a delayed splash plays once the flight time elapses', () => {
    playFishingSound('splash', 1400)

    expect(playedCount('splash')).toBe(0)
    vi.advanceTimersByTime(1400)
    expect(playedCount('splash')).toBe(1)
  })

  it('an aborted cast cancels the pending splash — no splash after the line is back in', () => {
    playFishingSound('splash', 1400)

    cancelPendingFishingSounds()

    vi.advanceTimersByTime(5000)
    expect(playedCount('splash')).toBe(0)
  })

  it('cancels every pending timer, whoosh included, when aborted inside the swing delay', () => {
    playFishingSound('cast', 200)
    playFishingSound('splash', 1400)

    cancelPendingFishingSounds()

    vi.advanceTimersByTime(5000)
    expect(playedCount('cast')).toBe(0)
    expect(playedCount('splash')).toBe(0)
  })

  it('cancel does not touch sounds that already played', () => {
    playFishingSound('splash', 1400)
    vi.advanceTimersByTime(1400)

    cancelPendingFishingSounds()

    expect(playedCount('splash')).toBe(1)
  })

  it('immediate sounds are unaffected by a pending-timer cancel', () => {
    cancelPendingFishingSounds()
    playFishingSound('plop')

    expect(playedCount('plop')).toBe(1)
  })
})

describe('dungeon drip audio', () => {
  beforeEach(() => {
    playbacks = []
    vi.stubGlobal('Audio', FakeAudio)
    sfxVolume.set(0.5)
    sfxMuted.set(false)
  })

  afterEach(() => {
    stopDungeonDripSounds()
    sfxVolume.set(0.5)
    sfxMuted.set(false)
    vi.unstubAllGlobals()
  })

  it('fades with distance and skips drops outside hearing range', () => {
    playDungeonDripSound(0, 1.5)
    playDungeonDripSound(5, 1.5)
    playDungeonDripSound(10, 1.5)
    playDungeonDripSound(25, 1.5)
    expect(playbacks).toHaveLength(2)
    expect(playbacks[0].volume).toBeCloseTo(0.175)
    expect(playbacks[1].volume).toBeCloseTo(playbacks[0].volume / 4)
  })

  it('honors the SFX volume and mute settings', () => {
    sfxMuted.set(true)
    playDungeonDripSound(0, 1)
    sfxMuted.set(false)
    sfxVolume.set(0)
    playDungeonDripSound(0, 1)
    expect(playbacks).toHaveLength(0)
    sfxVolume.set(0.2)
    playDungeonDripSound(0, 1)
    expect(playbacks[0].volume).toBeCloseTo(0.07)
  })

  it('varies the pitch between puddles', () => {
    playDungeonDripSound(0, 1.1)
    playDungeonDripSound(0, 1.9)
    expect(playbacks[0].rate).not.toBe(playbacks[1].rate)
  })

  it('stops drip tails on floor exit without stopping other effects', () => {
    playDungeonDripSound(0, 1)
    playFishingSound('plop')
    stopDungeonDripSounds()
    expect(playbacks[0].audio.paused).toBe(true)
    expect(playbacks[1].audio.paused).toBe(false)
  })
})
