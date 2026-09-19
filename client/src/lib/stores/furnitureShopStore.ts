import { derived, get, writable } from 'svelte/store'
import shop from '../../../../data/furniture_shop.json'
import type { EstateChest } from '../network/networkTypes'

export { shop as furnitureShop }
export type FurnitureProduct = (typeof shop.products)[number]
export type FurnitureBasketLine = { displayId: number; quantity: number }
export const furnitureBasket = writable<FurnitureBasketLine[]>([])
export const furnitureShopHover = writable<{
  displayId: number
  product: FurnitureProduct
} | null>(null)
export const furnitureCheckoutOpen = writable(false)
export const furnitureAtCheckout = writable(false)
export const furniturePurchasePending = writable(false)
export const furnitureShopError = writable<string | null>(null)
export const estateSignEditor = writable<EstateChest | null>(null)
export const furnitureBasketTotal = derived(furnitureBasket, (lines) =>
  lines.reduce(
    (total, line) =>
      total + (displayProduct(line.displayId)?.price ?? 0) * line.quantity,
    0
  )
)

export function displayProduct(displayId: number) {
  return shop.products.find((product) => product.displayIds.includes(displayId))
}

export function addFurnitureToBasket(displayId: number) {
  if (get(furniturePurchasePending) || !displayProduct(displayId)) return
  const lines = get(furnitureBasket)
  if (lines.reduce((total, line) => total + line.quantity, 0) >= 64) {
    furnitureShopError.set('Your basket holds at most 64 pieces.')
    return
  }
  const existing = lines.find((line) => line.displayId === displayId)
  furnitureBasket.set(
    existing
      ? lines.map((line) =>
          line === existing ? { ...line, quantity: line.quantity + 1 } : line
        )
      : [...lines, { displayId, quantity: 1 }]
  )
  furnitureShopError.set(null)
}

export function removeFurnitureFromBasket(displayId: number) {
  if (get(furniturePurchasePending)) return
  furnitureBasket.update((lines) =>
    lines.flatMap((line) =>
      line.displayId !== displayId
        ? [line]
        : line.quantity > 1
          ? [{ ...line, quantity: line.quantity - 1 }]
          : []
    )
  )
}

export function clearFurnitureBasket() {
  furnitureBasket.set([])
  furnitureCheckoutOpen.set(false)
  furnitureShopError.set(null)
}

export function resetFurnitureShop() {
  clearFurnitureBasket()
  furniturePurchasePending.set(false)
  furnitureAtCheckout.set(false)
  furnitureShopHover.set(null)
  estateSignEditor.set(null)
}
