import type { LobbySettingsPreset, TournamentGameMode } from '@/types/tournament'
import { CONVAR_SLIDERS, PRESET_BASE_CONVARS } from './options'

export interface SliderState {
  enabled: boolean
  value: number
}

export interface GameRulesState {
  gameMode: TournamentGameMode
  preset: LobbySettingsPreset
  customJson: string
  sliders: Record<string, SliderState>
  autoLobby: boolean
  matchObjective: string
}

export type LobbyPayload =
  | { ok: true; preset: LobbySettingsPreset; settings: Record<string, unknown> | undefined }
  | { ok: false; error: string }

export function initSliders(): Record<string, SliderState> {
  return Object.fromEntries(
    CONVAR_SLIDERS.map((c) => [c.key, { enabled: false, value: c.defaultValue }]),
  )
}

export function defaultGameRules(): GameRulesState {
  return {
    gameMode: 'standard',
    preset: 'standard',
    customJson: '',
    sliders: initSliders(),
    autoLobby: true,
    matchObjective: 'auto',
  }
}

export function hasActiveSliders(sliders: Record<string, SliderState>): boolean {
  return CONVAR_SLIDERS.some((c) => sliders[c.key]?.enabled)
}

function parseCustomJson(text: string): Record<string, unknown> | string {
  try {
    const parsed: unknown = JSON.parse(text)
    if (typeof parsed !== 'object' || parsed === null || Array.isArray(parsed)) {
      return 'Muss ein JSON-Objekt sein, z. B. {"sv_gravity": 200}'
    }
    return parsed as Record<string, unknown>
  } catch {
    return 'Ungültiges JSON'
  }
}

export function mergeConvars(
  preset: LobbySettingsPreset,
  custom: Record<string, unknown> | undefined,
  sliders: Record<string, SliderState>,
): Record<string, unknown> | null {
  const active = CONVAR_SLIDERS.filter((c) => sliders[c.key]?.enabled).map((c) => [
    c.key,
    sliders[c.key].value,
  ])
  if (active.length === 0) return null
  const base = preset === 'custom' ? { ...(custom ?? {}) } : { ...PRESET_BASE_CONVARS[preset] }
  return { ...base, ...Object.fromEntries(active) }
}

export function resolveLobbyPayload(rules: GameRulesState): LobbyPayload {
  let custom: Record<string, unknown> | undefined
  if (rules.preset === 'custom') {
    const parsed = parseCustomJson(rules.customJson)
    if (typeof parsed === 'string') return { ok: false, error: parsed }
    custom = parsed
  }
  const merged = mergeConvars(rules.preset, custom, rules.sliders)
  if (merged) return { ok: true, preset: 'custom', settings: merged }
  return { ok: true, preset: rules.preset, settings: rules.preset === 'custom' ? custom : undefined }
}

export function previewConvars(rules: GameRulesState): Record<string, unknown> {
  const parsed = rules.preset === 'custom' ? parseCustomJson(rules.customJson) : undefined
  const custom = typeof parsed === 'object' ? parsed : undefined
  return mergeConvars(rules.preset, custom, rules.sliders) ?? PRESET_BASE_CONVARS[rules.preset]
}

function sameConvars(left: Record<string, unknown>, right: Record<string, unknown>): boolean {
  const leftKeys = Object.keys(left)
  if (leftKeys.length !== Object.keys(right).length) return false
  return leftKeys.every((key) => left[key] === right[key])
}

function storedConvars(stored: string | null | undefined): Record<string, unknown> | null {
  if (!stored) return {}
  const parsed = parseCustomJson(stored)
  return typeof parsed === 'string' ? null : parsed
}

export function lobbyFromStored(
  stored: string | null | undefined,
): Pick<GameRulesState, 'preset' | 'customJson' | 'sliders'> {
  const convars = storedConvars(stored)
  const presets = Object.keys(PRESET_BASE_CONVARS) as LobbySettingsPreset[]
  const match = convars
    ? presets.find((key) => key !== 'custom' && sameConvars(PRESET_BASE_CONVARS[key], convars))
    : undefined
  if (match) return { preset: match, customJson: '', sliders: initSliders() }
  return {
    preset: 'custom',
    customJson: convars ? JSON.stringify(convars, null, 2) : stored ?? '',
    sliders: initSliders(),
  }
}

export function lobbyChanged(current: GameRulesState, initial: GameRulesState): boolean {
  const pick = (rules: GameRulesState) =>
    JSON.stringify([rules.preset, rules.customJson, rules.sliders])
  return pick(current) !== pick(initial)
}
