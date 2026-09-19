import { beforeEach, describe, expect, it } from 'vitest'
import { get } from 'svelte/store'
import {
  addFurnitureToBasket,
  removeFurnitureFromBasket,
  furnitureBasket,
  furnitureBasketTotal,
  furniturePurchasePending,
  furnitureShopError,
  resetFurnitureShop,
} from './furnitureShopStore'

describe('showroom basket', () => {
  beforeEach(resetFurnitureShop)

  it('counts displayed decorations while excluding the store sign', () => {
    addFurnitureToBasket(84)
    expect(get(furnitureBasket)).toEqual([])
    addFurnitureToBasket(103)
    addFurnitureToBasket(103)
    addFurnitureToBasket(106)
    expect(get(furnitureBasketTotal)).toBe(600)
    removeFurnitureFromBasket(103)
    expect(get(furnitureBasketTotal)).toBe(400)
    removeFurnitureFromBasket(103)
    expect(get(furnitureBasket)).toEqual([{ displayId: 106, quantity: 1 }])
  })

  it('limits the basket across products and keeps pending orders stable', () => {
    for (let i = 0; i < 63; i++) addFurnitureToBasket(103)
    addFurnitureToBasket(106)
    addFurnitureToBasket(107)
    expect(get(furnitureBasketTotal)).toBe(12800)
    expect(get(furnitureShopError)).not.toBeNull()
    furniturePurchasePending.set(true)
    removeFurnitureFromBasket(103)
    addFurnitureToBasket(103)
    expect(get(furnitureBasketTotal)).toBe(12800)
    resetFurnitureShop()
    expect(get(furnitureBasket)).toEqual([])
    expect(get(furniturePurchasePending)).toBe(false)
    expect(get(furnitureShopError)).toBeNull()
  })
})
