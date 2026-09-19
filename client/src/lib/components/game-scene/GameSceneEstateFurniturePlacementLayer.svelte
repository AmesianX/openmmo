<script lang="ts">
  import { T, useTask, useThrelte } from '@threlte/core'
  import { onMount } from 'svelte'
  import { get } from 'svelte/store'
  import * as THREE from 'three'
  import { loadEstateFurnitureModel } from '../../utils/estateFurnitureModels'
  import {
    cameraRotationEnabled,
    housingEditorMode,
    mapEditorMode,
  } from '../../stores/debugStore'
  import { currentDungeonDepth } from '../../stores/dungeonStore'
  import {
    estateFurniturePlacementRotation,
    rotateEstateFurniturePlacement,
  } from '../../stores/estateFurniturePlacementStore'
  import { isTypingTarget } from '../../utils/dom'
  import {
    EstatePlacementGrid,
    furnitureFootprintsOverlap,
    furniturePlacementRotation,
    footprintOnOwnedEstate,
    footprintOnHouseFloor,
    houseFloorY,
    housingPlacementFloor,
    housingPlacementHouseId,
    pointOnHouseFloor,
    snapPlacementCoordinate,
    type EstateFurniturePlacement,
    type EstateFurniturePlacementDefinition,
    type EstatePlot,
    type FurnitureFootprintPose,
  } from '../../terrain/estatePlacement'
  import { wrapWorldX } from '../../terrain/world-wrap'
  import type { TerrainHeightManager } from '../../managers/terrainHeightManager'
  import { housingManager } from '../../managers/housingManager'
  import { playerInsideHouseId } from '../../stores/housingStore'
  import type { LocalPlayer } from '../../stores/gameStore'

  let {
    definition,
    plots,
    pending,
    terrainMeshes,
    housingGroup,
    furnitureGroup,
    ignoreFurnitureId,
    heightManager,
    player,
    floorLevel,
    obstacles = [],
    onplace,
    oncancel,
    onerror,
  }: {
    definition: EstateFurniturePlacementDefinition
    plots: EstatePlot[]
    pending: boolean
    terrainMeshes: (THREE.Mesh | undefined)[]
    housingGroup?: THREE.Group
    furnitureGroup?: THREE.Group
    ignoreFurnitureId?: number
    heightManager: TerrainHeightManager
    player: LocalPlayer | null
    floorLevel: number
    obstacles?: FurnitureFootprintPose[]
    onplace: (placement: EstateFurniturePlacement) => void
    oncancel: () => void
    onerror: (message: string) => void
  } = $props()

  const { camera, renderer } = useThrelte()
  const group = new THREE.Group()
  const raycaster = new THREE.Raycaster()
  const pointer = new THREE.Vector2()
  function placementHouse() {
    const houseId = get(playerInsideHouseId)
    return houseId ? housingManager.getHouseById(houseId) : undefined
  }

  const grid = new EstatePlacementGrid(
    (x, z) => {
      const house = placementHouse()
      if (house) return houseFloorY(house, floorLevel, x, z)
      return floorLevel === 0 ? heightManager.groundYOrNull(x, z) : null
    },
    (x, z) => {
      const house = placementHouse()
      return !house || pointOnHouseFloor(house, floorLevel, x, z)
    }
  )
  group.add(grid.object)

  let ghost: THREE.Group | null = null
  let cursor: { x: number; y: number } | null = null
  let heightOffset = 0
  let preview: EstateFurniturePlacement | null = null
  let previewValid = false
  let lastGridFloor = Infinity
  let lastGridHouseId: string | null = null
  let disposed = false

  function setPointer(clientX: number, clientY: number) {
    const rect = renderer.domElement.getBoundingClientRect()
    pointer.set(
      ((clientX - rect.left) / rect.width) * 2 - 1,
      -((clientY - rect.top) / rect.height) * 2 + 1
    )
    raycaster.setFromCamera(pointer, get(camera))
  }

  function hidePreview() {
    if (ghost) ghost.visible = false
    preview = null
    previewValid = false
  }

  function surfaceHit() {
    const surfaces: THREE.Object3D[] = terrainMeshes.filter(
      (mesh): mesh is THREE.Mesh => !!mesh
    )
    if (housingGroup) surfaces.push(housingGroup)
    if (furnitureGroup && definition.maxHeightOffset)
      surfaces.push(furnitureGroup)
    return raycaster.intersectObjects(surfaces, true).find((hit) => {
      if (!hit.face) return false
      if (ignoreFurnitureId !== undefined) {
        for (
          let object: THREE.Object3D | null = hit.object;
          object;
          object = object.parent
        ) {
          if (object.userData.estateChestId === ignoreFurnitureId) return false
        }
      }
      const upward =
        hit.face.normal.clone().transformDirection(hit.object.matrixWorld).y >
        0.65
      if (!upward) return false
      const housingFloor = housingPlacementFloor(hit.object)
      const insideHouseId = get(playerInsideHouseId)
      if (housingFloor !== null)
        return (
          insideHouseId !== null &&
          housingFloor === floorLevel &&
          housingPlacementHouseId(hit.object) === insideHouseId
        )
      let parent: THREE.Object3D | null = hit.object
      while (parent) {
        if (parent === furnitureGroup) return true
        if (parent === housingGroup) return false
        parent = parent.parent
      }
      return floorLevel === 0 && insideHouseId === null
    })
  }

  function tintGhost(valid: boolean) {
    ghost?.traverse((object) => {
      if (!(object instanceof THREE.Mesh)) return
      const materials = Array.isArray(object.material)
        ? object.material
        : [object.material]
      for (const material of materials) {
        if (material instanceof THREE.MeshStandardMaterial)
          material.color.set(valid ? '#65d98a' : '#ef5b5b')
      }
    })
  }

  function updatePreview() {
    if (!player || !ghost || !cursor || get(cameraRotationEnabled)) {
      hidePreview()
      return
    }
    setPointer(cursor.x, cursor.y)
    const hit = surfaceHit()
    if (!hit) {
      hidePreview()
      return
    }
    const x = snapPlacementCoordinate(hit.point.x, definition.snapStep)
    const z = snapPlacementCoordinate(hit.point.z, definition.snapStep)
    const house = placementHouse()
    const fits = (rotation: number) =>
      footprintOnOwnedEstate(x, z, rotation, definition.footprint, plots) &&
      (definition.solid === false ||
        !obstacles.some((obstacle) =>
          furnitureFootprintsOverlap(
            { x, z, rotationDeg: rotation, footprint: definition.footprint },
            obstacle
          )
        )) &&
      (!house ||
        footprintOnHouseFloor(
          house,
          floorLevel,
          x,
          z,
          rotation,
          definition.footprint,
          definition.floorEdgeClearance
        ))
    const rotation = get(estateFurniturePlacementRotation)
    const rotationDeg = furniturePlacementRotation(
      rotation.degrees,
      definition.rotationStep,
      rotation.manual,
      fits
    )
    if (rotationDeg !== rotation.degrees) {
      estateFurniturePlacementRotation.set({
        ...rotation,
        degrees: rotationDeg,
      })
    }
    const baseY =
      (house
        ? houseFloorY(house, floorLevel, x, z)
        : heightManager.groundYOrNull(x, z)) ?? hit.point.y
    const y = definition.maxHeightOffset
      ? Math.max(
          baseY,
          Math.min(
            baseY + definition.maxHeightOffset,
            hit.point.y + heightOffset
          )
        )
      : baseY
    previewValid =
      floorLevel >= definition.minFloor &&
      floorLevel <= definition.maxFloor &&
      fits(rotationDeg)
    ghost.visible = true
    ghost.position.set(x, y, z)
    ghost.rotation.y = THREE.MathUtils.degToRad(rotationDeg)
    tintGhost(previewValid)
    preview = {
      position: { x: wrapWorldX(x), y, z },
      rotationDeg,
      floorLevel,
    }
  }

  onMount(() => {
    const unsubscribeHeight = heightManager.onHeightChanged(() =>
      grid.markDirty()
    )
    const unsubscribeHouses = housingManager.onHousesChanged(() =>
      grid.markDirty()
    )
    loadEstateFurnitureModel(definition)
      .then((gltf) => {
        if (disposed) return
        ghost = gltf.scene.clone(true)
        ghost.visible = false
        ghost.renderOrder = 5
        ghost.traverse((object) => {
          if (!(object instanceof THREE.Mesh)) return
          const sources = Array.isArray(object.material)
            ? object.material
            : [object.material]
          const materials = sources.map((source) => {
            const material = source.clone()
            material.transparent = true
            material.opacity = 0.55
            material.depthWrite = false
            return material
          })
          object.material = Array.isArray(object.material)
            ? materials
            : materials[0]
        })
        group.add(ghost)
      })
      .catch((error) => {
        console.error('Failed to load estate furniture preview:', error)
        onerror('Could not load the furniture preview. Reload to try again.')
      })

    const canvas = renderer.domElement
    const move = (event: PointerEvent) => {
      cursor = { x: event.clientX, y: event.clientY }
    }
    const leave = () => {
      cursor = null
      hidePreview()
    }
    const click = (event: MouseEvent) => {
      if (
        event.button !== 0 ||
        pending ||
        get(cameraRotationEnabled) ||
        get(mapEditorMode) ||
        get(housingEditorMode) ||
        get(currentDungeonDepth) > 0
      )
        return
      event.preventDefault()
      event.stopImmediatePropagation()
      cursor = { x: event.clientX, y: event.clientY }
      updatePreview()
      if (preview && previewValid) onplace(preview)
    }
    const wheel = (event: WheelEvent) => {
      if (pending || event.deltaY === 0 || get(cameraRotationEnabled)) return
      event.preventDefault()
      event.stopImmediatePropagation()
      const direction = event.deltaY > 0 ? 1 : -1
      if (event.shiftKey && definition.maxHeightOffset) {
        heightOffset = Math.max(
          -3,
          Math.min(3, heightOffset - direction * 0.05)
        )
      } else {
        rotateEstateFurniturePlacement(direction)
      }
      updatePreview()
    }
    const keydown = (event: KeyboardEvent) => {
      if (isTypingTarget(event.target)) return
      if (
        event.code === 'KeyR' &&
        !event.ctrlKey &&
        !event.metaKey &&
        !event.altKey &&
        !get(cameraRotationEnabled)
      ) {
        event.preventDefault()
        event.stopImmediatePropagation()
        if (!pending && !event.repeat) {
          rotateEstateFurniturePlacement(event.shiftKey ? -1 : 1)
          updatePreview()
        }
        return
      }
      if (event.code !== 'Escape') return
      event.preventDefault()
      event.stopImmediatePropagation()
      if (pending) return
      oncancel()
      hidePreview()
    }
    canvas.addEventListener('pointermove', move)
    canvas.addEventListener('pointerleave', leave)
    canvas.addEventListener('mousedown', click, true)
    canvas.addEventListener('wheel', wheel, { capture: true, passive: false })
    window.addEventListener('keydown', keydown, true)
    return () => {
      disposed = true
      unsubscribeHeight()
      unsubscribeHouses()
      canvas.removeEventListener('pointermove', move)
      canvas.removeEventListener('pointerleave', leave)
      canvas.removeEventListener('mousedown', click, true)
      canvas.removeEventListener('wheel', wheel, true)
      window.removeEventListener('keydown', keydown, true)
      ghost?.traverse((object) => {
        if (!(object instanceof THREE.Mesh)) return
        const materials = Array.isArray(object.material)
          ? object.material
          : [object.material]
        for (const material of materials) material.dispose()
      })
      grid.dispose()
    }
  })

  useTask(() => {
    group.visible = get(currentDungeonDepth) < 1
    const insideHouseId = get(playerInsideHouseId)
    if (floorLevel !== lastGridFloor || insideHouseId !== lastGridHouseId) {
      lastGridFloor = floorLevel
      lastGridHouseId = insideHouseId
      grid.markDirty()
    }
    grid.update(!!player, plots, player?.position.x ?? 0)
    updatePreview()
  })
</script>

<T is={group} />
