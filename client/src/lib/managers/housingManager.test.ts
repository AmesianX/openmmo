import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { HouseData } from '../types/housing'

const removed: string[] = []
vi.mock('../wasm/onlinerpg_shared', () => ({
  passability_add_house: vi.fn(),
  passability_remove_house: (id: string) => removed.push(id),
  passability_update_door: vi.fn(),
  passability_is_movement_blocked: () => false,
  passability_is_circle_blocked: () => false,
}))
const { HousingManager } = await import('./housingManager')

function house(id: string, x = 0): HouseData {
  return {
    id,
    origin: { x, y: 0, z: 0 },
    rooms: [],
    passability: [{ floorLevel: 0, cells: [] }],
  } as unknown as HouseData
}

describe('house subscriptions', () => {
  let manager: InstanceType<typeof HousingManager>
  beforeEach(() => {
    manager = new HousingManager()
    removed.length = 0
  })

  it('waits for the complete snapshot, including an empty one', async () => {
    let ready = false
    const waiting = manager.waitForSnapshot().then(() => {
      ready = true
    })
    await Promise.resolve()
    expect(ready).toBe(false)
    manager.completeSnapshot()
    await waiting
    expect(manager.isSynchronized()).toBe(true)
  })

  it('removes rendering and collision together on leave and reset', () => {
    manager.handleRemoteHousesBatch([house('a'), house('b')])
    manager.completeSnapshot()
    manager.handleRemoteHouseRemoved('a')
    expect(manager.getAllHouses().map((h) => h.id)).toEqual(['b'])
    expect(removed).toEqual(['a'])
    manager.resetView()
    expect(manager.getAllHouses()).toEqual([])
    expect(removed).toEqual(['a', 'b'])
    expect(manager.isSynchronized()).toBe(false)
  })

  it('replaces a returning house with the server snapshot', () => {
    manager.handleRemoteHouseSpawned(house('a', 0))
    manager.handleRemoteHouseRemoved('a')
    manager.handleRemoteHouseSpawned(house('a', 90))
    expect(manager.getHouseById('a')?.origin.x).toBe(90)
  })
})
