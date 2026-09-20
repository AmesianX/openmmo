import { readFileSync } from 'node:fs'
import { get, writable } from 'svelte/store'
import { afterEach, beforeAll, beforeEach, expect, it, vi } from 'vitest'
import type { ItemInstance } from '../network/networkTypes'
import { networkManager } from '../network/socket'
import { abilityCooldowns, resetAbilities } from '../stores/abilityStore'
import {
  acknowledgeDaggerSkill,
  daggerSkillState,
  resetDaggerSkill,
} from '../stores/daggerSkillStore'
import {
  gameStore,
  reportSkillFailure,
  type LocalPlayer,
} from '../stores/gameStore'
import { inspectionTargeting, resetInspection } from '../stores/inspectionStore'
import { inventoryStore } from '../stores/inventoryStore'
import { manaState } from '../stores/manaStore'
import { quickslots, resetQuickslots } from '../stores/quickslotStore'
import { initSync } from '../wasm/onlinerpg_shared'
import { useAbility } from './useAbility'

vi.mock('../stores/gameStore', () => ({
  gameStore: writable({ currentPlayer: null }),
  hoveredMonsterId: writable(null),
  reportSkillFailure: vi.fn(),
}))
vi.mock('../network/socket', () => ({
  networkManager: { sendUseAbility: vi.fn() },
}))
vi.mock('../managers/combatController', () => ({
  combatController: { getAbilityTarget: vi.fn() },
}))
vi.mock('../managers/monsterManager', () => ({
  monsterManager: { monsters: new Map() },
}))

const item = (item_def_id: string): ItemInstance => ({
  instance_id: 1,
  item_def_id,
  enchant: 0,
  quantity: 1,
})

function updatePlayer(patch: Partial<LocalPlayer>) {
  gameStore.update((state) => ({
    ...state,
    currentPlayer: { ...state.currentPlayer!, ...patch },
  }))
}

beforeAll(() => {
  initSync({
    module: readFileSync(
      new URL('../wasm/onlinerpg_shared_bg.wasm', import.meta.url)
    ),
  })
})

beforeEach(() => {
  vi.clearAllMocks()
  vi.useFakeTimers()
  vi.setSystemTime(1000)
  resetAbilities()
  resetDaggerSkill()
  resetInspection()
  resetQuickslots()
  updatePlayer({ characterClass: 'knight', health: 10, mount: null })
  inventoryStore.set({
    bag: [],
    equipped: {
      main_hand: item('iron_sword'),
      off_hand: item('wooden_shield'),
      neck: item('stethoscope'),
    },
  })
  manaState.set({ mana: 10, max_mana: 10 })
})

afterEach(() => vi.useRealTimers())

it('casts without a quickslot and suppresses duplicate requests while pending', () => {
  useAbility('guardian_ward')
  useAbility('guardian_ward')
  expect(networkManager.sendUseAbility).toHaveBeenCalledExactlyOnceWith(
    'guardian_ward',
    null
  )
  expect(get(quickslots).every((slot) => slot === null)).toBe(true)
  expect(reportSkillFailure).not.toHaveBeenCalled()
})

it.each([
  [{ health: 0 }, 'You cannot use skills while dead.'],
  [{ mount: 'horse' }, 'You cannot use skills while mounted.'],
] as const)('rejects unavailable player states: %j', (patch, message) => {
  updatePlayer(patch)
  useAbility('guardian_ward')
  expect(networkManager.sendUseAbility).not.toHaveBeenCalled()
  expect(reportSkillFailure).toHaveBeenCalledWith(message)
})

it('keeps equipment errors for missing skill items', () => {
  inventoryStore.set({ bag: [], equipped: {} })
  useAbility('auscultation')
  expect(reportSkillFailure).toHaveBeenCalledWith(
    'Equip a stethoscope to use Auscultation.'
  )
  expect(get(inspectionTargeting)).toBe(false)
  useAbility('guardian_ward')
  expect(reportSkillFailure).toHaveBeenLastCalledWith(
    'Cannot use Guardian Ward.'
  )
  expect(networkManager.sendUseAbility).not.toHaveBeenCalled()
})

it('rejects insufficient mana and honors the server cooldown', () => {
  manaState.set({ mana: 1, max_mana: 10 })
  useAbility('guardian_ward')
  expect(reportSkillFailure).toHaveBeenLastCalledWith('Not enough mana.')
  manaState.set({ mana: 2, max_mana: 10 })
  abilityCooldowns.set({ guardian_ward: 2000 })
  useAbility('guardian_ward')
  expect(reportSkillFailure).toHaveBeenLastCalledWith(
    'Guardian Ward is not ready yet.'
  )
  expect(networkManager.sendUseAbility).not.toHaveBeenCalled()
  vi.setSystemTime(2000)
  useAbility('guardian_ward')
  expect(networkManager.sendUseAbility).toHaveBeenCalledOnce()
})

it('queues and cancels inspection, and cancels it when using another skill', () => {
  useAbility('auscultation')
  expect(get(inspectionTargeting)).toBe(true)
  expect(networkManager.sendUseAbility).not.toHaveBeenCalled()
  useAbility('auscultation')
  expect(get(inspectionTargeting)).toBe(false)
  useAbility('auscultation')
  useAbility('guardian_ward')
  expect(get(inspectionTargeting)).toBe(false)
  expect(networkManager.sendUseAbility).toHaveBeenCalledOnce()
})

it('queues Double Slash for the next attack, toggles it off, and honors its cooldown', () => {
  updatePlayer({ characterClass: 'rogue' })
  inventoryStore.set({ bag: [], equipped: { main_hand: item('dagger') } })
  manaState.set({ mana: 0, max_mana: 10 })
  useAbility('dagger_double_slash')
  expect(get(daggerSkillState).queued).toBe(true)
  useAbility('dagger_double_slash')
  expect(get(daggerSkillState).queued).toBe(false)
  acknowledgeDaggerSkill(10000)
  useAbility('dagger_double_slash')
  expect(get(daggerSkillState).queued).toBe(false)
  expect(reportSkillFailure).toHaveBeenCalledWith(
    'Double Slash is not ready yet.'
  )
  expect(networkManager.sendUseAbility).not.toHaveBeenCalled()
})

it('ignores unknown, hidden, and other-class skills', () => {
  for (const id of ['unknown', 'radiance', 'bow_mark', 'dagger_double_slash'])
    useAbility(id)
  expect(networkManager.sendUseAbility).not.toHaveBeenCalled()
  expect(get(daggerSkillState).queued).toBe(false)
  expect(reportSkillFailure).not.toHaveBeenCalled()
})
