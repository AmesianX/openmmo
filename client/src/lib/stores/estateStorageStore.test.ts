import { beforeEach, describe, expect, it, vi } from 'vitest'
import { get } from 'svelte/store'

vi.mock('../wasm/onlinerpg_shared', () => ({
  passability_set_furniture: vi.fn(),
}))

import { passability_set_furniture } from '../wasm/onlinerpg_shared'
import {
  selectedEstateFurniture,
  estateFurniturePlacementMode,
  estateFurniturePlacementRotation,
  startEstateFurniturePlacement,
  rotateEstateFurniturePlacement,
  stopEstateFurniturePlacement,
} from './estateFurniturePlacementStore'
import {
  applyEstateChestVisibility,
  estateChests,
  openEstateChest,
  resetEstateStorage,
} from './estateStorageStore'

const chest = {
  id: 7,
  estate_id: 2,
  owner_id: 3,
  item_def_id: 'storage_chest',
  position: { x: 2.5, y: 5, z: 2.5 },
  rotation_deg: 90,
  floor_level: 0,
  overdue: false,
  revision: 0,
}

describe('estate storage visibility', () => {
  beforeEach(() => {
    resetEstateStorage()
    vi.clearAllMocks()
  })

  it('keeps contents private while syncing the visible solid chest', () => {
    applyEstateChestVisibility([chest], [])

    expect([...get(estateChests).values()]).toEqual([chest])
    expect(get(openEstateChest)).toBeNull()
    expect(passability_set_furniture).toHaveBeenLastCalledWith(
      'furniture:estate-storage:0,0',
      [
        {
          id: 7,
          type: 'chest_animated',
          x: 2.5,
          y: 5,
          z: 2.5,
          rotation: 90,
          floorLevel: 0,
        },
      ]
    )
  })

  it('removal clears collision and closes the matching window', () => {
    applyEstateChestVisibility([chest], [])
    selectedEstateFurniture.set(chest)
    startEstateFurniturePlacement({
      kind: 'move',
      furniture: chest,
      item_def_id: chest.item_def_id,
      owner_id: chest.owner_id,
      plots: [{ x: 0, z: 0 }],
    })
    openEstateChest.set({
      chest_id: chest.id,
      item_def_id: 'storage_chest',
      revision: 0,
      max_weight: 500,
      can_deposit: true,
      items: [],
    })
    vi.mocked(passability_set_furniture).mockClear()

    applyEstateChestVisibility([], [chest.id])

    expect(get(estateChests).size).toBe(0)
    expect(get(openEstateChest)).toBeNull()
    expect(get(selectedEstateFurniture)).toBeNull()
    expect(get(estateFurniturePlacementMode)).toBeNull()
    expect(passability_set_furniture).toHaveBeenLastCalledWith(
      'furniture:estate-storage:0,0',
      []
    )
  })

  it('cancels a rotated move without changing the original furniture', () => {
    applyEstateChestVisibility([chest], [])
    selectedEstateFurniture.set(chest)
    startEstateFurniturePlacement({
      kind: 'move',
      furniture: chest,
      item_def_id: chest.item_def_id,
      owner_id: chest.owner_id,
      plots: [{ x: 0, z: 0 }],
    })
    expect(get(estateFurniturePlacementRotation)).toEqual({
      degrees: 90,
      manual: true,
    })
    rotateEstateFurniturePlacement(1)
    expect(get(estateFurniturePlacementRotation).degrees).toBe(180)
    stopEstateFurniturePlacement()
    expect(get(estateChests).get(chest.id)).toEqual(chest)
    expect(get(estateFurniturePlacementMode)).toBeNull()
    expect(get(selectedEstateFurniture)).toBeNull()
  })

  it('moves collision between buckets and refreshes the selected furniture', () => {
    applyEstateChestVisibility([chest], [])
    selectedEstateFurniture.set(chest)
    const moved = { ...chest, revision: 1, position: { x: 34.5, y: 5, z: 2.5 } }
    vi.mocked(passability_set_furniture).mockClear()
    applyEstateChestVisibility([moved], [])
    expect(get(estateChests).size).toBe(1)
    expect(get(selectedEstateFurniture)).toEqual(moved)
    expect(passability_set_furniture).toHaveBeenCalledWith(
      'furniture:estate-storage:0,0',
      []
    )
    expect(passability_set_furniture).toHaveBeenCalledWith(
      'furniture:estate-storage:1,0',
      [expect.objectContaining({ id: chest.id, x: 34.5 })]
    )
  })
})
