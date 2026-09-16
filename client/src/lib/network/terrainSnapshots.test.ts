import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { TerrainSnapshots } from './terrainSnapshots'

describe('terrain HTTP snapshots', () => {
  let replies: Array<(response: Response) => void>
  let fetchMock: ReturnType<typeof vi.fn>
  let apply: ReturnType<typeof vi.fn<(tile: number) => void>>
  let resync: ReturnType<typeof vi.fn<() => void>>
  let tiles: TerrainSnapshots<number>
  const version = (value: string) => ({ tile_x: 1, tile_z: 2, version: value })
  const reply = (value: number) => new Response(new Uint8Array([value]))
  const settle = () => vi.advanceTimersByTimeAsync(0)

  beforeEach(() => {
    vi.useFakeTimers()
    replies = []
    fetchMock = vi.fn(
      () => new Promise<Response>((resolve) => replies.push(resolve))
    )
    vi.stubGlobal('fetch', fetchMock)
    apply = vi.fn()
    resync = vi.fn()
    tiles = new TerrainSnapshots(
      () => '',
      (bytes) => bytes[0],
      apply,
      resync
    )
  })

  afterEach(() => {
    tiles.reset()
    vi.unstubAllGlobals()
    vi.useRealTimers()
  })

  it('applies only the latest requested version when responses arrive out of order', async () => {
    tiles.set(version('old'))
    tiles.set(version('new'))
    replies[1](reply(2))
    await settle()
    replies[0](reply(1))
    await settle()
    expect(apply.mock.calls).toEqual([[2]])
  })

  it('ignores a response after leaving, then reuses it on reentry', async () => {
    tiles.set(version('one'))
    tiles.remove('1,2')
    replies[0](reply(1))
    await settle()
    expect(apply).not.toHaveBeenCalled()
    tiles.set(version('one'))
    await settle()
    expect(apply).toHaveBeenCalledWith(1)
    expect(fetchMock).toHaveBeenCalledTimes(1)
  })

  it('deduplicates downloads across resets without applying the old subscription', async () => {
    tiles.set(version('one'))
    tiles.reset()
    tiles.set(version('one'))
    replies[0](reply(1))
    await settle()
    expect(fetchMock).toHaveBeenCalledTimes(1)
    expect(apply.mock.calls).toEqual([[1]])
  })

  it('reuses content after a reset but does not apply an abandoned request', async () => {
    tiles.set(version('one'))
    tiles.reset()
    replies[0](reply(1))
    await settle()
    expect(apply).not.toHaveBeenCalled()
    tiles.set(version('one'))
    expect(apply).toHaveBeenCalledWith(1)
    expect(fetchMock).toHaveBeenCalledTimes(1)
  })

  it('requests current versions when the origin no longer has an old version', async () => {
    tiles.set(version('one'))
    replies[0](new Response(null, { status: 409 }))
    await settle()
    expect(resync).toHaveBeenCalledOnce()
    expect(apply).not.toHaveBeenCalled()
  })

  it('retries temporary failures and cancels retries on reset', async () => {
    tiles.set(version('one'))
    replies[0](new Response(null, { status: 503 }))
    await settle()
    await vi.advanceTimersByTimeAsync(1000)
    expect(fetchMock).toHaveBeenCalledTimes(2)
    replies[1](new Response(null, { status: 503 }))
    await settle()
    tiles.reset()
    await vi.advanceTimersByTimeAsync(1000)
    expect(fetchMock).toHaveBeenCalledTimes(2)
    expect(apply).not.toHaveBeenCalled()
  })
})
