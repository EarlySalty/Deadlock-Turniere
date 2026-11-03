import { LEAGUE_LABEL } from '@/lib/league'
import type {
  InviteMode,
  LobbySettingsPreset,
  TournamentGameMode,
  TournamentMode,
} from '@/types/tournament'

export const INPUT_CLASS =
  'w-full rounded-lg border border-border bg-background px-3 py-2 text-foreground placeholder:text-muted focus:outline-none focus:ring-2 focus:ring-primary/50 disabled:opacity-60'

export interface Choice<T extends string> {
  value: T
  label: string
  description: string
}

export type TournamentFormat = 'auto' | TournamentMode

export const MODE_OPTIONS: Choice<TournamentMode>[] = [
  {
    value: 'bracket_only',
    label: 'Nur Bracket',
    description: 'K.-o.-System ohne Vorrunde. Wer verliert, scheidet aus (bei Double Elimination nach der zweiten Niederlage).',
  },
  {
    value: 'group_stage',
    label: 'Gruppenphase + Bracket',
    description: 'Erst spielen alle in Gruppen, die Besten jeder Gruppe ziehen ins K.-o.-Bracket ein.',
  },
  {
    value: 'league',
    label: LEAGUE_LABEL,
    description: 'Jedes Team spielt einmal gegen jedes andere, Runde für Runde zu festen Zeiten. Sieg +2, Niederlage -1. Am Ende entscheidet die Tabelle, ein Bracket gibt es nicht.',
  },
]

export const FORMAT_OPTIONS: Choice<TournamentFormat>[] = [
  {
    value: 'auto',
    label: 'Automatisch',
    description: 'Je nach Teamgröße und Anmeldungen wählt das System Gruppenphase + Bracket oder nur Bracket.',
  },
  ...MODE_OPTIONS,
]

export const GAME_MODE_OPTIONS: Choice<TournamentGameMode>[] = [
  {
    value: 'standard',
    label: 'Standard',
    description: 'Normales Match, alle wählen ihren Helden frei.',
  },
  {
    value: 'mirror',
    label: 'Mirror Match',
    description: 'Der Bot lost pro Match für jedes Team einen eigenen Helden aus, das ganze Team spielt ihn (z. B. 6× Yamato gegen 6× Calico).',
  },
  {
    value: 'all_same',
    label: 'All Same Hero',
    description: 'Alle Spieler beider Teams spielen denselben zufälligen Helden.',
  },
  {
    value: 'random_heroes',
    label: 'Random Heroes',
    description: 'Jeder Spieler bekommt einen zufälligen Helden, ohne Doppelungen, solange genug Helden da sind.',
  },
  {
    value: 'single_lane',
    label: 'Single Lane Battle',
    description: 'In Vorbereitung: Der Bot kündigt den Modus nur an und schränkt die Lanes noch nicht ein. Heldenwahl frei.',
  },
]

export const INVITE_MODE_OPTIONS: Choice<InviteMode>[] = [
  {
    value: 'always',
    label: 'Einladungen jederzeit',
    description: 'Captains können angemeldete Spieler während der ganzen Anmeldung in ihr Team einladen.',
  },
  {
    value: 'window',
    label: 'Einladungen nur im Zeitfenster',
    description: 'Captains können nur zwischen den beiden Zeitpunkten unten einladen. Ohne beide Zeitpunkte ist keine Einladung möglich.',
  },
  {
    value: 'never',
    label: 'Keine Einladungen',
    description: 'Captains können niemanden einladen. Teams gründen, beitreten und sich bewerben geht weiterhin.',
  },
]

export const MATCH_OBJECTIVE_OPTIONS: { value: string; label: string }[] = [
  { value: 'auto', label: 'Automatisch (nach Teamgröße)' },
  { value: 'first_guardian', label: 'Erster Guardian gewinnt' },
  { value: 'first_walker', label: 'Erster Walker gewinnt' },
  { value: 'base', label: 'Reguläres Spielende (Basis)' },
]

export const PRESET_LABELS: Record<LobbySettingsPreset, string> = {
  standard: 'Standard',
  fast_mode: 'Fast Mode (schnelle Cooldowns)',
  high_damage: 'High Damage (2× DPS)',
  low_gravity: 'Low Gravity (Mondschwerkraft)',
  speed_mode: 'Speed Mode (2× Bewegung + schnelle Cooldowns)',
  glass_cannon: 'Glass Cannon (5× Schaden, jeder stirbt sofort)',
  rich_start: 'Rich Start (10.000 Gold beim Start)',
  chaos_mode: 'Chaos Mode (weniger Schwerkraft, schneller, mehr Schaden, viel Gold)',
  all_same_hero: 'All Same Hero (Duplikate erlaubt)',
  immortal: 'Immortal (kein Heldentod)',
  custom: 'Eigene Werte (JSON)',
}

export const PRESET_BASE_CONVARS: Record<LobbySettingsPreset, Record<string, number>> = {
  standard: {},
  fast_mode: { citadel_enable_fast_cooldowns: 1 },
  high_damage: { citadel_dps_multiplier: 2 },
  low_gravity: { sv_gravity: 200 },
  speed_mode: { citadel_player_move_speed_scale: 2.0, citadel_enable_fast_cooldowns: 1 },
  glass_cannon: { citadel_weapon_damage_multiplier: 5, citadel_dps_multiplier: 3, citadel_melee_damage_scale: 3.0 },
  rich_start: { citadel_player_starting_gold: 10000 },
  chaos_mode: { sv_gravity: 400, citadel_player_move_speed_scale: 1.5, citadel_weapon_damage_multiplier: 2, citadel_enable_fast_cooldowns: 1, citadel_player_starting_gold: 5000, citadel_trooper_gold_reward: 200 },
  all_same_hero: { citadel_allow_duplicate_heroes: 1 },
  immortal: { citadel_enable_no_hero_death: 1 },
  custom: {},
}

export interface ConvarSliderConfig {
  key: string
  label: string
  min: number
  max: number
  step: number
  defaultValue: number
  unit: string
  hint: string
}

export const CONVAR_SLIDERS: ConvarSliderConfig[] = [
  { key: 'sv_gravity', label: 'Schwerkraft', min: 50, max: 800, step: 10, defaultValue: 800, unit: '', hint: 'Normal: 800. Niedriger heißt höhere Sprünge und mehr Zeit in der Luft.' },
  { key: 'host_timescale', label: 'Spielgeschwindigkeit', min: 0.1, max: 3.0, step: 0.1, defaultValue: 1.0, unit: '×', hint: 'Normal: 1.0. Unter 1 Zeitlupe, über 1 Zeitraffer.' },
  { key: 'citadel_player_move_speed_scale', label: 'Bewegungsgeschwindigkeit', min: 0.5, max: 3.0, step: 0.1, defaultValue: 1.0, unit: '×', hint: 'Normal: 1.0. Gilt für alle Spieler.' },
  { key: 'citadel_weapon_damage_multiplier', label: 'Waffenschaden', min: 0.5, max: 10, step: 0.5, defaultValue: 1.0, unit: '×', hint: 'Normal: 1.0.' },
  { key: 'citadel_dps_multiplier', label: 'DPS-Multiplikator', min: 0.5, max: 5, step: 0.5, defaultValue: 1.0, unit: '×', hint: 'Normal: 1.0.' },
]

export function optionDescription<T extends string>(options: Choice<T>[], value: T): string {
  return options.find((option) => option.value === value)?.description ?? ''
}

export const DATE_HINTS = {
  registrationEnd: 'Gleichzeitig Anmeldeschluss. Die Erinnerungen richten sich danach.',
  checkinStart: 'Leer: Check-in startet mit dem Turnier-Start.',
}
