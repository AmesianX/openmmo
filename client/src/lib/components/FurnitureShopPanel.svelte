<script lang="ts">
  import {
    furnitureBasket,
    furnitureBasketTotal,
    furnitureShopHover,
    furnitureCheckoutOpen,
    furnitureAtCheckout,
    furniturePurchasePending,
    furnitureShopError,
    clearFurnitureBasket,
  } from '../stores/furnitureShopStore'
  import { playerGold } from '../stores/inventoryStore'
  import { itemDisplayName } from '../data/itemDefs'
  import { networkManager } from '../network/socket'
  import FurnitureBasket from './FurnitureBasket.svelte'
  import GoldAmount from './GoldAmount.svelte'

  function checkout() {
    if ($furniturePurchasePending || !$furnitureAtCheckout) return
    furniturePurchasePending.set(true)
    furnitureShopError.set(null)
    networkManager.sendCheckoutFurniture(
      $furnitureBasket.map((line) => ({
        display_id: line.displayId,
        quantity: line.quantity,
      })),
      $playerGold,
      $furnitureBasketTotal
    )
  }
</script>

{#if $furnitureShopHover || $furnitureBasket.length || $furnitureShopError}
  <div class="shop-panel">
    {#if $furnitureShopHover}
      <p>
        {itemDisplayName($furnitureShopHover.product.itemDefId)} · <GoldAmount
          copper={$furnitureShopHover.product.price}
        />
      </p>
      <small>Click the display to add one to your unpaid basket.</small>
    {/if}
    {#if $furnitureCheckoutOpen}
      <FurnitureBasket />
      <p>Your gold: <GoldAmount copper={$playerGold} /></p>
      <button
        disabled={$furniturePurchasePending ||
          !$furnitureAtCheckout ||
          !$furnitureBasket.length}
        onclick={checkout}
        >{$furniturePurchasePending
          ? 'Paying…'
          : 'Pay and take furniture'}</button
      >
      <button
        disabled={$furniturePurchasePending}
        onclick={clearFurnitureBasket}>Return all items</button
      >
      <button
        disabled={$furniturePurchasePending}
        onclick={() => furnitureCheckoutOpen.set(false)}>Keep browsing</button
      >
    {:else if $furnitureBasket.length}
      <p>
        {$furnitureBasket.reduce((n, line) => n + line.quantity, 0)} pieces · <GoldAmount
          copper={$furnitureBasketTotal}
        />
      </p>
      <button onclick={() => furnitureCheckoutOpen.set(true)}
        >Review basket</button
      >
      <small
        >Pay by the west entrance. Unpaid items are returned when you leave.</small
      >
    {/if}
    {#if $furnitureShopError}<p role="status">{$furnitureShopError}</p>{/if}
  </div>
{/if}

<style>
  .shop-panel {
    position: fixed;
    z-index: 150;
    right: 20px;
    bottom: 100px;
    width: 330px;
    backdrop-filter: blur(4px);
    padding: 10px;
    border: 1px solid rgba(255, 255, 255, 0.18);
    border-radius: 10px;
    background: rgba(6, 10, 14, 0.88);
    color: #e6edf3;
    font-family: 'Courier New', monospace;
    font-size: 12px;
    max-width: calc(100vw - 64px);
  }
  small {
    display: block;
    margin-top: 8px;
    color: #c9bea1;
  }
  button {
    margin: 8px 6px 0 0;
    padding: 6px 9px;
    cursor: pointer;
  }
  .shop-panel p {
    margin: 8px 0;
  }
  .shop-panel small {
    color: #9fb2c3;
    font-size: 11px;
  }
  .shop-panel button {
    background: none;
    border: 1px solid rgba(255, 255, 255, 0.2);
    border-radius: 4px;
    color: #9fb2c3;
    font-family: inherit;
    font-size: 11px;
    font-weight: 700;
    padding: 2px 6px;
  }
  .shop-panel button:hover:not(:disabled) {
    color: #fff;
    border-color: rgba(255, 255, 255, 0.4);
  }
  .shop-panel button:disabled {
    opacity: 0.4;
    cursor: default;
  }
  .shop-panel [role='status'] {
    color: #f0b8b8;
  }
</style>
