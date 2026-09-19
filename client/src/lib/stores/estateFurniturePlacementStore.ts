import { get, writable } from 'svelte/store'
import type { EstatePlot } from '../terrain/estatePlacement'
import { getEstateStorageDef } from '../data/estateFurnitureDefs'
import type { EstateChest } from '../network/networkTypes'

export type EstateFurniturePlacementMode = {
  item_def_id: string
  owner_id: number
  plots: EstatePlot[]
} & (
  | { kind: 'place'; instance_id: number }
  | { kind: 'move'; furniture: EstateChest }
)

export const selectedEstateFurniture = writable<EstateChest | null>(null)
export const estateFurniturePlacementMode =
  writable<EstateFurniturePlacementMode | null>(null)
export const estateFurniturePlacementPending = writable(false)
export const estateFurniturePlacementError = writable<string | null>(null)
export const estateFurniturePlacementRotation = writable({
  degrees: 0,
  manual: false,
})

export function startEstateFurniturePlacement(
  mode: EstateFurniturePlacementMode
) {
  estateFurniturePlacementRotation.set({
    degrees: mode.kind === 'move' ? mode.furniture.rotation_deg : 0,
    manual: mode.kind === 'move',
  })
  estateFurniturePlacementPending.set(false)
  estateFurniturePlacementError.set(null)
  estateFurniturePlacementMode.set(mode)
}

export function rotateEstateFurniturePlacement(direction: 1 | -1 = 1) {
  const definition = getEstateStorageDef(
    get(estateFurniturePlacementMode)?.item_def_id
  )
  if (!definition || get(estateFurniturePlacementPending)) return
  estateFurniturePlacementRotation.update(({ degrees }) => ({
    degrees: (degrees + direction * definition.rotationStep + 360) % 360,
    manual: true,
  }))
  estateFurniturePlacementError.set(null)
}

export function stopEstateFurniturePlacement() {
  selectedEstateFurniture.set(null)
  estateFurniturePlacementMode.set(null)
  estateFurniturePlacementPending.set(false)
  estateFurniturePlacementError.set(null)
  estateFurniturePlacementRotation.set({ degrees: 0, manual: false })
}
