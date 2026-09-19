<script lang="ts">
  import {
    landscapingMode,
    landscapingPending,
    landscapingError,
    landscapingHint,
    hasLandscapingToolbox,
    selectLandscapingTool,
  } from '../stores/landscapingStore'
  import {
    fenceMode,
    fencePending,
    fenceError,
    fenceTarget,
    fenceCount,
    showFenceNoSpawnZones,
    stopFenceMode,
  } from '../stores/fenceStore'
  import {
    estateChestError,
    estateChestMode,
    estateChestPending,
    stopEstateChestMode,
  } from '../stores/estateStorageStore'
  import { inventoryStore } from '../stores/inventoryStore'
  import {
    estateFurniturePlacementRotation,
    rotateEstateFurniturePlacement,
    selectedEstateFurniture,
  } from '../stores/estateFurniturePlacementStore'
  import { estateStorageDefs } from '../data/estateFurnitureDefs'
  import { getItemDef, itemDisplayName } from '../data/itemDefs'
  import { playerVisualFloorLevel } from '../stores/housingStore'
  import { networkManager } from '../network/socket'
  import type { LandscapingTool } from '../terrain/landscaping'
  import { isAdminUser } from '../stores/gameStore'
  import { stopHouseInteraction } from '../stores/housePlacementStore'
  import HousePlacementPanel from './HousePlacementPanel.svelte'
  import SplatBrushPanel from './map-editor/SplatBrushPanel.svelte'
  import { draggablePanel } from '../actions/draggablePanel'

  type EditorTab = Exclude<LandscapingTool, 'Fence'> | 'Objects'

  let panelElement = $state<HTMLDivElement>()
  const selectedFurnitureId = $derived($selectedEstateFurniture?.id)
  $effect(() => {
    if (selectedFurnitureId !== undefined && panelElement)
      panelElement.scrollTop = 0
  })

  const tabs: EditorTab[] = ['Ground', 'Road', 'Objects', 'House']
  const editorOpen = $derived(
    $landscapingMode !== null ||
      $estateChestMode !== null ||
      $selectedEstateFurniture !== null
  )
  const activeTab = $derived<EditorTab>(
    $estateChestMode ||
      $selectedEstateFurniture ||
      $landscapingMode?.tool === 'Fence'
      ? 'Objects'
      : ($landscapingMode?.tool ?? 'Objects')
  )
  const fenceDefinition = getItemDef('wooden_fence')
  const rotationStep = $derived(
    estateStorageDefs.get($estateChestMode?.item_def_id ?? '')?.rotationStep ??
      90
  )
  const storageObjects = $derived(
    [...estateStorageDefs.values()].map((definition) => {
      const items = $inventoryStore.bag.filter(
        (item) => item.item_def_id === definition.itemDefId
      )
      return {
        definition: getItemDef(definition.itemDefId),
        itemDefId: definition.itemDefId,
        instanceId: items[0]?.instance_id,
        quantity: items.reduce((total, item) => total + item.quantity, 0),
      }
    })
  )

  function selectTab(tab: EditorTab) {
    if (tab === activeTab || $estateChestPending) return
    if (tab === 'Objects') {
      selectFence()
      return
    }
    if (tab !== 'House') {
      stopHouseInteraction()
    }
    if ($estateChestMode || $selectedEstateFurniture) {
      stopEstateChestMode()
      networkManager.sendStartLandscapingMode(tab)
    } else selectLandscapingTool(tab)
  }

  function selectFence() {
    if ($fenceMode || $estateChestPending) return
    stopHouseInteraction()
    stopEstateChestMode()
    if ($landscapingMode) {
      selectLandscapingTool('Fence')
    } else {
      networkManager.sendStartLandscapingMode('Fence')
    }
  }

  function selectStorage(instanceId: number | undefined) {
    if (instanceId === undefined || $estateChestPending) return
    stopHouseInteraction()
    networkManager.sendUseItem(instanceId)
  }

  function editSelected(action: 'move' | 'recover') {
    const selected = $selectedEstateFurniture
    if (!selected || $estateChestPending) return
    estateChestPending.set(true)
    estateChestError.set(null)
    if (action === 'move')
      networkManager.sendStartEstateFurnitureMove(selected.id)
    else networkManager.sendRecoverEstateChest(selected.id)
  }

  function close() {
    if ($estateChestPending) return
    stopHouseInteraction()
    stopFenceMode()
    stopEstateChestMode()
  }
</script>

{#if editorOpen}
  {@const status =
    $estateChestMode || $selectedEstateFurniture
      ? $estateChestPending
        ? 'Saving…'
        : ($estateChestError ??
          ($estateChestMode
            ? 'Point inside your estate and click to place'
            : 'Choose Move or Recover for the selected furniture'))
      : $landscapingMode?.tool === 'House'
        ? null
        : $landscapingMode?.tool === 'Fence'
          ? $fencePending
            ? 'Saving…'
            : ($fenceError ?? $fenceTarget?.reason)
          : $landscapingPending
            ? 'Saving…'
            : ($landscapingError ?? $landscapingHint)}
  <div
    class="landscaping-panel"
    bind:this={panelElement}
    use:draggablePanel={'landscaping'}
  >
    <div class="panel-header" data-drag-handle>
      <strong>Estate Editor</strong>
      <button
        class="close-btn"
        disabled={$estateChestPending}
        aria-label="Close estate editor"
        title="Close (Esc)"
        onclick={close}>×</button
      >
    </div>
    <div class="tabs" role="tablist" aria-label="Estate editing tools">
      {#each tabs as tab (tab)}
        <button
          role="tab"
          aria-selected={activeTab === tab}
          class:active={activeTab === tab}
          disabled={$estateChestPending ||
            (tab !== 'Objects' && !$hasLandscapingToolbox)}
          title={tab !== 'Objects' && !$hasLandscapingToolbox
            ? "Carry a Landscaper's Toolbox to use this tool"
            : tab}
          onclick={() => selectTab(tab)}>{tab}</button
        >
      {/each}
    </div>
    {#if activeTab === 'House'}
      <HousePlacementPanel />
    {:else if activeTab === 'Objects'}
      <div class="object-content">
        {#if $selectedEstateFurniture}
          <div class="selected-furniture">
            <strong
              >{itemDisplayName($selectedEstateFurniture.item_def_id)}</strong
            >
            {#if $estateChestMode?.kind === 'move'}
              <small>Choose a new position. Esc cancels the move.</small>
              <button
                disabled={$estateChestPending}
                onclick={stopEstateChestMode}>Cancel move</button
              >
            {:else}
              <div class="furniture-actions">
                <button
                  disabled={$estateChestPending}
                  onclick={() => editSelected('move')}>Move</button
                >
                <button
                  disabled={$estateChestPending}
                  onclick={() => editSelected('recover')}>Recover</button
                >
                <button
                  disabled={$estateChestPending}
                  onclick={stopEstateChestMode}>Cancel</button
                >
              </div>
              <small
                >Move keeps stored items and sign text. Empty storage before
                recovering.</small
              >
            {/if}
          </div>
        {/if}
        <strong>Placeable Objects</strong>
        {#if $estateChestMode}
          <div class="rotation-controls">
            <button
              disabled={$estateChestPending}
              aria-label="Rotate left {rotationStep} degrees"
              title="Rotate left (Shift + R)"
              onclick={() => rotateEstateFurniturePlacement(-1)}
              >↶ {rotationStep}°</button
            >
            <span>Rotation {$estateFurniturePlacementRotation.degrees}°</span>
            <button
              disabled={$estateChestPending}
              aria-label="Rotate right {rotationStep} degrees"
              title="Rotate right (R)"
              onclick={() => rotateEstateFurniturePlacement(1)}
              >↷ {rotationStep}°</button
            >
          </div>
        {/if}
        <div class="object-list">
          <button
            class="object-row"
            class:active={$fenceMode !== null}
            title={$fenceCount
              ? 'Place or recover fences'
              : 'Select to recover placed fences'}
            onclick={selectFence}
          >
            {#if fenceDefinition}
              <img src="/items/{fenceDefinition.icon}" alt="" />
            {/if}
            <span>{itemDisplayName('wooden_fence')}</span>
            <small>×{$fenceCount}</small>
          </button>
          {#each storageObjects as object (object.itemDefId)}
            <button
              class="object-row"
              class:active={$estateChestMode?.item_def_id === object.itemDefId}
              disabled={object.instanceId === undefined || $estateChestPending}
              title={object.quantity
                ? `Place ${itemDisplayName(object.itemDefId)}`
                : 'None in your bag'}
              onclick={() => selectStorage(object.instanceId)}
            >
              {#if object.definition}
                <img src="/items/{object.definition.icon}" alt="" />
              {/if}
              <span>{itemDisplayName(object.itemDefId)}</span>
              <small>×{object.quantity}</small>
            </button>
          {/each}
        </div>
        {#if $isAdminUser && $fenceMode}
          <label class="zone-toggle">
            <input type="checkbox" bind:checked={$showFenceNoSpawnZones} />
            Show no-spawn zones
          </label>
        {/if}
        {#if $estateChestMode}
          <small
            >{$playerVisualFloorLevel + 1}F · Left-click to place · Right-click
            to move · R / Shift + R or mouse wheel rotates {rotationStep}° ·
            Shift + wheel adjusts decoration height · Right-click placed
            furniture to move or recover · Esc to finish</small
          >
        {:else if $fenceMode}
          <small
            >Left-click to place or recover · Right-click to move · Esc to
            finish</small
          >
        {/if}
        {#if !$estateChestMode}
          <small
            >Right-click placed furniture nearby to move or recover it.</small
          >
        {/if}
      </div>
    {:else}
      <SplatBrushPanel
        sizeLabel={activeTab === 'Road' ? 'Width' : 'Size'}
        title={activeTab === 'Ground' ? 'Ground Brush' : 'Road Tool'}
        hint={activeTab === 'Ground' ? '(drag to paint)' : '(click two points)'}
        availableLayers={$landscapingMode?.palette ?? []}
      />
    {/if}
    {#if status}
      <div class="paint-status" role="status">{status}</div>
    {/if}
  </div>
{/if}

<style>
  .landscaping-panel {
    position: fixed;
    bottom: 100px;
    left: 16px;
    z-index: 40;
    max-width: calc(100vw - 32px);
    max-height: calc(100vh - 120px);
    overflow: auto;
    scrollbar-width: thin;
    scrollbar-color: rgba(171, 147, 103, 0.5) transparent;
    border-radius: 8px;
    background: #211c16ed;
    color: #f3e8d2;
    pointer-events: auto;
  }
  .landscaping-panel::-webkit-scrollbar {
    width: 6px;
    height: 6px;
  }
  .landscaping-panel::-webkit-scrollbar-track {
    background: transparent;
  }
  .landscaping-panel::-webkit-scrollbar-thumb {
    background: rgba(171, 147, 103, 0.5);
    border-radius: 999px;
  }
  .landscaping-panel::-webkit-scrollbar-thumb:hover {
    background: rgba(205, 178, 128, 0.7);
  }
  .landscaping-panel::-webkit-scrollbar-corner {
    background: transparent;
  }
  .panel-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 4px 8px;
    font-family: 'Courier New', monospace;
    font-size: 13px;
  }
  button {
    cursor: pointer;
    color: inherit;
    background: #3a3024;
    border: 1px solid #766247;
    border-radius: 4px;
    padding: 5px 14px;
    font-family: 'Courier New', monospace;
    font-size: 12px;
    font-weight: bold;
  }
  .close-btn {
    display: grid;
    place-items: center;
    width: 24px;
    height: 24px;
    padding: 0;
    border: none;
    background: transparent;
    font-size: 16px;
    line-height: 1;
  }
  .close-btn:hover {
    background: #3a3024;
  }
  button:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }
  .tabs {
    display: flex;
    gap: 2px;
    margin: 0 0 4px;
    padding: 2px;
    background: rgba(0, 0, 0, 0.7);
  }
  .tabs button {
    flex: 1;
    padding: 4px 10px;
    border: none;
    background: transparent;
    color: #888;
    letter-spacing: 0.5px;
    transition:
      background 150ms ease,
      color 150ms ease;
  }
  .tabs button:hover:not(:disabled) {
    color: #ccc;
  }
  .tabs button.active {
    background: rgba(226, 185, 59, 0.25);
    color: #e2b93b;
  }
  .landscaping-panel :global(.splat-brush-panel) {
    border: none;
    border-block: 1px solid rgba(226, 185, 59, 0.3);
    border-radius: 0;
    box-shadow: none;
  }
  .object-content,
  .paint-status {
    display: grid;
    gap: 6px;
    padding: 12px 16px;
    font-family: 'Courier New', monospace;
    font-size: 12px;
  }
  .object-list {
    display: grid;
    gap: 5px;
    min-width: 280px;
  }
  .selected-furniture {
    display: grid;
    gap: 8px;
    padding: 10px;
    border: 1px solid rgba(226, 185, 59, 0.3);
    border-radius: 6px;
    background: rgba(226, 185, 59, 0.08);
  }
  .selected-furniture small {
    max-width: 300px;
    color: #aaa;
  }
  .furniture-actions {
    display: flex;
    gap: 6px;
  }
  .rotation-controls {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }
  .object-row {
    display: grid;
    grid-template-columns: 28px 1fr auto;
    align-items: center;
    gap: 8px;
    padding: 6px 8px;
    text-align: left;
  }
  .object-row.active {
    border-color: #e2b93b;
    background: rgba(226, 185, 59, 0.2);
  }
  .object-row img {
    width: 28px;
    height: 28px;
    object-fit: contain;
  }
  .object-row small,
  .object-content > small {
    color: #aaa;
  }
  .zone-toggle {
    display: flex;
    align-items: center;
    gap: 6px;
    cursor: pointer;
  }
  .zone-toggle input {
    accent-color: #e2b93b;
  }
</style>
