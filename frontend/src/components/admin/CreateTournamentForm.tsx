import { useState } from 'react'
import type { FormEvent } from 'react'
import Card from '@/components/ui/Card'
import Button from '@/components/ui/Button'
import DateTimeInput from '@/components/ui/DateTimeInput'
import { useCreateTournament } from '@/hooks/useTournament'
import type {
  BracketFormat,
  InviteMode,
  LobbySettingsPreset,
  TournamentGameMode,
} from '@/types/tournament'
import { Trophy, AlertCircle, CheckCircle, SlidersHorizontal, Swords } from 'lucide-react'

const GAME_MODE_OPTIONS: {
  value: TournamentGameMode
  label: string
  description: string
}[] = [
  {
    value: 'standard',
    label: 'Standard',
    description: 'Normales Match — Spieler wählen ihren Helden frei.',
  },
  {
    value: 'mirror',
    label: 'Mirror Match',
    description: 'Bot würfelt pro Team einen Hero. Beide Teams spielen denselben Hero (z.B. 6× Yamato vs 6× Calico).',
  },
  {
    value: 'all_same',
    label: 'All Same Hero',
    description: 'Alle 12 Spieler spielen exakt denselben zufälligen Hero.',
  },
  {
    value: 'random_heroes',
    label: 'Random Heroes',
    description: 'Jedem Spieler wird ein zufälliger Hero zugewiesen — unique solange Pool reicht.',
  },
  {
    value: 'single_lane',
    label: 'Single Lane Battle',
    description: '6v6 nur auf einer Lane. Heldenwahl frei. (In Vorbereitung.)',
  },
]

// ---------------------------------------------------------------------------
// Preset-Metadaten (spiegeln das Backend)
// ---------------------------------------------------------------------------

const PRESET_LABELS: Record<LobbySettingsPreset, string> = {
  standard:    'Standard',
  fast_mode:   'Fast Mode (schnelle Cooldowns)',
  high_damage: 'High Damage (2× DPS)',
  low_gravity: 'Low Gravity (Mondschwerchkraft)',
  speed_mode:  'Speed Mode (2× Bewegung + schnelle Cooldowns)',
  glass_cannon:'Glass Cannon (5× Schaden, jeder stirbt sofort)',
  rich_start:  'Rich Start (10.000 Gold beim Start)',
  chaos_mode:  'Chaos Mode (weniger Schwerkraft, schneller, mehr Schaden, viel Gold)',
  all_same_hero:'All Same Hero (Duplikate erlaubt)',
  immortal:    'Immortal (kein Heldentod)',
  custom:      'Custom (manuelles JSON)',
}

// Basis-Convars je Preset — muss mit dem Backend übereinstimmen
const PRESET_BASE_CONVARS: Record<LobbySettingsPreset, Record<string, number>> = {
  standard:    {},
  fast_mode:   { citadel_enable_fast_cooldowns: 1 },
  high_damage: { citadel_dps_multiplier: 2 },
  low_gravity: { sv_gravity: 200 },
  speed_mode:  { citadel_player_move_speed_scale: 2.0, citadel_enable_fast_cooldowns: 1 },
  glass_cannon:{ citadel_weapon_damage_multiplier: 5, citadel_dps_multiplier: 3, citadel_melee_damage_scale: 3.0 },
  rich_start:  { citadel_player_starting_gold: 10000 },
  chaos_mode:  { sv_gravity: 400, citadel_player_move_speed_scale: 1.5, citadel_weapon_damage_multiplier: 2, citadel_enable_fast_cooldowns: 1, citadel_player_starting_gold: 5000, citadel_trooper_gold_reward: 200 },
  all_same_hero:{ citadel_allow_duplicate_heroes: 1 },
  immortal:    { citadel_enable_no_hero_death: 1 },
  custom:      {},
}

// ---------------------------------------------------------------------------
// Konfigurierbare Regler (alle OFF by default)
// ---------------------------------------------------------------------------

interface ConvarSliderConfig {
  key: string
  label: string
  min: number
  max: number
  step: number
  defaultValue: number
  unit: string
  hint: string
}

const CONVAR_SLIDERS: ConvarSliderConfig[] = [
  {
    key: 'sv_gravity',
    label: 'Schwerkraft',
    min: 50,
    max: 800,
    step: 10,
    defaultValue: 800,
    unit: '',
    hint: 'Standard: 800 — niedriger = höhere Sprünge, mehr Luftzeit',
  },
  {
    key: 'host_timescale',
    label: 'Spielgeschwindigkeit',
    min: 0.1,
    max: 3.0,
    step: 0.1,
    defaultValue: 1.0,
    unit: '×',
    hint: 'Standard: 1.0 — unter 1 = Zeitlupe, über 1 = Zeitraffer',
  },
  {
    key: 'citadel_player_move_speed_scale',
    label: 'Bewegungsgeschwindigkeit',
    min: 0.5,
    max: 3.0,
    step: 0.1,
    defaultValue: 1.0,
    unit: '×',
    hint: 'Standard: 1.0 — gilt für alle Spieler',
  },
  {
    key: 'citadel_weapon_damage_multiplier',
    label: 'Waffenschaden',
    min: 0.5,
    max: 10,
    step: 0.5,
    defaultValue: 1.0,
    unit: '×',
    hint: 'Standard: 1.0',
  },
  {
    key: 'citadel_dps_multiplier',
    label: 'DPS-Multiplikator',
    min: 0.5,
    max: 5,
    step: 0.5,
    defaultValue: 1.0,
    unit: '×',
    hint: 'Standard: 1.0',
  },
]

interface SliderState {
  enabled: boolean
  value: number
}

function initSliders(): Record<string, SliderState> {
  return Object.fromEntries(
    CONVAR_SLIDERS.map((c) => [c.key, { enabled: false, value: c.defaultValue }])
  )
}

// ---------------------------------------------------------------------------
// Hilfsfunktion: Preset-Basis + aktive Slider zusammenführen
// ---------------------------------------------------------------------------

function buildFinalConvars(
  preset: LobbySettingsPreset,
  customJsonParsed: Record<string, unknown> | undefined,
  sliders: Record<string, SliderState>,
): Record<string, unknown> | null {
  const activeSliderEntries = CONVAR_SLIDERS
    .filter((c) => sliders[c.key]?.enabled)
    .map((c) => [c.key, sliders[c.key].value])

  if (activeSliderEntries.length === 0) return null

  const base: Record<string, unknown> =
    preset === 'custom'
      ? { ...(customJsonParsed ?? {}) }
      : { ...PRESET_BASE_CONVARS[preset] }

  return { ...base, ...Object.fromEntries(activeSliderEntries) }
}

function parseReminderOffsets(value: string): number[] {
  const parsed = value
    .split(',')
    .map((entry) => Number(entry.trim()))
    .filter((entry) => Number.isFinite(entry) && entry >= 0)
  return parsed.length > 0 ? parsed : [1440, 120, 15]
}

// ---------------------------------------------------------------------------
// Formular
// ---------------------------------------------------------------------------

export default function CreateTournamentForm() {
  const [name, setName] = useState('')
  const [description, setDescription] = useState('')
  const [teamSize, setTeamSize] = useState(6)
  const [bracketFormat, setBracketFormat] = useState<BracketFormat>('single_elimination')
  const [seriesFormat, setSeriesFormat] = useState<1 | 3 | 5>(1)
  const [regStart, setRegStart] = useState('')
  const [regEnd, setRegEnd] = useState('')
  const [checkinStart, setCheckinStart] = useState('')
  const [inviteMode, setInviteMode] = useState<InviteMode>('always')
  const [inviteWindowStart, setInviteWindowStart] = useState('')
  const [inviteWindowEnd, setInviteWindowEnd] = useState('')
  const [lobbyPreset, setLobbyPreset] = useState<LobbySettingsPreset>('standard')
  const [customJson, setCustomJson] = useState('')
  const [customJsonError, setCustomJsonError] = useState('')
  const [excludeFromLeaderboard, setExcludeFromLeaderboard] = useState(false)
  const [tournamentGameMode, setTournamentGameMode] =
    useState<TournamentGameMode>('standard')
  const [autoLobbyEnabled, setAutoLobbyEnabled] = useState(true)
  const [reminderOffsets, setReminderOffsets] = useState('1440, 120, 15')
  const [sliders, setSliders] = useState<Record<string, SliderState>>(initSliders)
  const [successMsg, setSuccessMsg] = useState('')

  const createMutation = useCreateTournament()

  const hasActiveSliders = CONVAR_SLIDERS.some((c) => sliders[c.key]?.enabled)

  const setSliderEnabled = (key: string, enabled: boolean) =>
    setSliders((prev) => ({ ...prev, [key]: { ...prev[key], enabled } }))

  const setSliderValue = (key: string, value: number) =>
    setSliders((prev) => ({ ...prev, [key]: { ...prev[key], value } }))

  const handleSubmit = (e: FormEvent) => {
    e.preventDefault()
    setSuccessMsg('')
    setCustomJsonError('')

    // Custom-JSON parsen (falls Preset = custom)
    let parsedCustom: Record<string, unknown> | undefined
    if (lobbyPreset === 'custom') {
      try {
        parsedCustom = JSON.parse(customJson)
        if (typeof parsedCustom !== 'object' || Array.isArray(parsedCustom) || parsedCustom === null) {
          setCustomJsonError('Muss ein JSON-Objekt sein, z.B. {"sv_gravity": 200}')
          return
        }
      } catch {
        setCustomJsonError('Ungültiges JSON')
        return
      }
    }

    // Wenn Regler aktiv: Preset-Basis + Regler zusammenführen → 'custom' senden
    const mergedConvars = buildFinalConvars(lobbyPreset, parsedCustom, sliders)
    const finalPreset: LobbySettingsPreset = mergedConvars ? 'custom' : lobbyPreset
    const finalSettings: Record<string, unknown> | undefined =
      mergedConvars ?? (lobbyPreset === 'custom' ? parsedCustom : undefined)

    createMutation.mutate(
      {
        name: name.trim(),
        description: description.trim() || undefined,
        team_size: teamSize,
        bracket_format: bracketFormat,
        series_format: seriesFormat,
        registration_start: regStart || undefined,
        registration_end: regEnd || undefined,
        checkin_start: checkinStart || undefined,
        invite_mode: inviteMode,
        invite_window_start: inviteMode === 'window' ? inviteWindowStart || undefined : undefined,
        invite_window_end: inviteMode === 'window' ? inviteWindowEnd || undefined : undefined,
        lobby_settings_preset: finalPreset,
        lobby_settings: finalSettings,
        tournament_game_mode: tournamentGameMode,
        auto_lobby_enabled: autoLobbyEnabled,
        exclude_from_leaderboard: excludeFromLeaderboard,
        reminder_offsets: parseReminderOffsets(reminderOffsets),
      },
      {
        onSuccess: (tournament) => {
          setSuccessMsg(`Turnier "${tournament.name}" wurde erfolgreich erstellt!`)
          setName('')
          setDescription('')
          setTeamSize(6)
          setBracketFormat('single_elimination')
          setSeriesFormat(1)
          setRegStart('')
          setRegEnd('')
          setCheckinStart('')
          setInviteMode('always')
          setInviteWindowStart('')
          setInviteWindowEnd('')
          setLobbyPreset('standard')
          setCustomJson('')
          setExcludeFromLeaderboard(false)
          setTournamentGameMode('standard')
          setAutoLobbyEnabled(true)
          setReminderOffsets('1440, 120, 15')
          setSliders(initSliders())
        },
      }
    )
  }

  return (
    <Card className="p-6">
      <div className="flex items-center gap-2 mb-6">
        <Trophy size={20} className="text-primary" />
        <h2 className="text-lg font-semibold text-foreground">Neues Turnier erstellen</h2>
      </div>

      <form onSubmit={handleSubmit} className="space-y-5">
        {/* Name */}
        <div>
          <label htmlFor="tournament-name" className="block text-sm font-medium text-foreground mb-1.5">
            Turniername *
          </label>
          <input
            id="tournament-name"
            type="text"
            required
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder="z.B. Deadlock Community Cup #1"
            className="w-full bg-background border border-border rounded-lg px-3 py-2 text-foreground placeholder:text-muted focus:outline-none focus:ring-2 focus:ring-primary/50"
          />
        </div>

        {/* Beschreibung */}
        <div>
          <label htmlFor="tournament-desc" className="block text-sm font-medium text-foreground mb-1.5">
            Beschreibung
          </label>
          <textarea
            id="tournament-desc"
            value={description}
            onChange={(e) => setDescription(e.target.value)}
            rows={3}
            placeholder="Optionale Beschreibung des Turniers..."
            className="w-full bg-background border border-border rounded-lg px-3 py-2 text-foreground placeholder:text-muted focus:outline-none focus:ring-2 focus:ring-primary/50 resize-none"
          />
        </div>

        <div className="grid grid-cols-1 sm:grid-cols-2 gap-4">
          {/* Teamgröße */}
          <div>
            <label htmlFor="team-size" className="block text-sm font-medium text-foreground mb-1.5">
              Teamgröße
            </label>
            <input
              id="team-size"
              type="number"
              min={1}
              max={20}
              value={teamSize}
              onChange={(e) => setTeamSize(Number(e.target.value))}
              className="w-full bg-background border border-border rounded-lg px-3 py-2 text-foreground focus:outline-none focus:ring-2 focus:ring-primary/50"
            />
          </div>

          {/* Bracket-Format */}
          <div>
            <label htmlFor="bracket-format" className="block text-sm font-medium text-foreground mb-1.5">
              Bracket-Format
            </label>
            <select
              id="bracket-format"
              value={bracketFormat}
              onChange={(e) => setBracketFormat(e.target.value as BracketFormat)}
              className="w-full bg-background border border-border rounded-lg px-3 py-2 text-foreground focus:outline-none focus:ring-2 focus:ring-primary/50"
            >
              <option value="single_elimination">Single Elimination</option>
              <option value="double_elimination">Double Elimination</option>
            </select>
          </div>
        </div>

        <div>
          <label htmlFor="series-format" className="block text-sm font-medium text-foreground mb-1.5">
            Serienformat
          </label>
          <select
            id="series-format"
            value={seriesFormat}
            onChange={(e) => setSeriesFormat(Number(e.target.value) as 1 | 3 | 5)}
            className="w-full bg-background border border-border rounded-lg px-3 py-2 text-foreground focus:outline-none focus:ring-2 focus:ring-primary/50"
          >
            <option value={1}>Bo1</option>
            <option value={3}>Bo3</option>
            <option value={5}>Bo5</option>
          </select>
        </div>

        <div className="grid grid-cols-1 sm:grid-cols-2 gap-4">
          <div>
            <label htmlFor="reg-start" className="block text-sm font-medium text-foreground mb-1.5">
              Anmeldung Start
            </label>
            <DateTimeInput
              id="reg-start"
              value={regStart}
              onChange={(e) => setRegStart(e.target.value)}
              className="w-full bg-background border border-border rounded-lg px-3 py-2 text-foreground focus:outline-none focus:ring-2 focus:ring-primary/50"
            />
          </div>
          <div>
            <label htmlFor="reg-end" className="block text-sm font-medium text-foreground mb-1.5">
              Turnier-Start
            </label>
            <DateTimeInput
              id="reg-end"
              value={regEnd}
              onChange={(e) => setRegEnd(e.target.value)}
              className="w-full bg-background border border-border rounded-lg px-3 py-2 text-foreground focus:outline-none focus:ring-2 focus:ring-primary/50"
            />
            <p className="text-xs text-muted mt-1">
              Anmeldeschluss = Turnier-Start. Reminder werden vor diesem Zeitpunkt verschickt.
            </p>
          </div>
          <div>
            <label htmlFor="checkin-start" className="block text-sm font-medium text-foreground mb-1.5">
              Check-in Start
            </label>
            <DateTimeInput
              id="checkin-start"
              value={checkinStart}
              onChange={(e) => setCheckinStart(e.target.value)}
              className="w-full bg-background border border-border rounded-lg px-3 py-2 text-foreground focus:outline-none focus:ring-2 focus:ring-primary/50"
            />
            <p className="text-xs text-muted mt-1">Leer = Check-in startet automatisch mit Turnier-Start.</p>
          </div>
        </div>

        <div className="grid grid-cols-1 sm:grid-cols-2 gap-4">
          <div>
            <label htmlFor="reminder-offsets" className="block text-sm font-medium text-foreground mb-1.5">
              Reminder vor Turnier-Start
            </label>
            <input
              id="reminder-offsets"
              type="text"
              value={reminderOffsets}
              onChange={(e) => setReminderOffsets(e.target.value)}
              placeholder="1440, 120, 15"
              className="w-full bg-background border border-border rounded-lg px-3 py-2 text-foreground placeholder:text-muted focus:outline-none focus:ring-2 focus:ring-primary/50"
            />
            <p className="mt-1 text-xs text-muted">Kommagetrennte Minuten, z.B. 1440, 120, 15</p>
          </div>

          <label className="flex items-center gap-3 rounded-lg border border-border px-3 py-2 cursor-pointer">
            <input
              type="checkbox"
              checked={excludeFromLeaderboard}
              onChange={(e) => setExcludeFromLeaderboard(e.target.checked)}
              className="h-4 w-4 accent-primary"
            />
            <div>
              <div className="text-sm font-medium text-foreground">Von Rangliste ausschließen</div>
              <div className="text-xs text-muted">Ideal für Testturniere oder interne Cups</div>
            </div>
          </label>
        </div>

        {/* Invite-Modus */}
        <div>
          <label htmlFor="invite-mode" className="block text-sm font-medium text-foreground mb-1.5">
            Einladungs-Modus
          </label>
          <select
            id="invite-mode"
            value={inviteMode}
            onChange={(e) => setInviteMode(e.target.value as InviteMode)}
            className="w-full bg-background border border-border rounded-lg px-3 py-2 text-foreground focus:outline-none focus:ring-2 focus:ring-primary/50"
          >
            <option value="always">Immer erlaubt</option>
            <option value="window">Nur im Zeitfenster</option>
            <option value="never">Deaktiviert (Auto-Balance)</option>
          </select>
          {inviteMode === 'never' && (
            <p className="mt-1 text-xs text-muted">
              Spieler werden beim Check-in-Abschluss automatisch rank-balanced auf Teams verteilt.
            </p>
          )}
        </div>

        {inviteMode === 'window' && (
          <div className="grid grid-cols-1 sm:grid-cols-2 gap-4">
            <div>
              <label htmlFor="invite-start" className="block text-sm font-medium text-foreground mb-1.5">
                Pick-Fenster Start
              </label>
              <DateTimeInput
                id="invite-start"
                value={inviteWindowStart}
                onChange={(e) => setInviteWindowStart(e.target.value)}
                className="w-full bg-background border border-border rounded-lg px-3 py-2 text-foreground focus:outline-none focus:ring-2 focus:ring-primary/50"
              />
            </div>
            <div>
              <label htmlFor="invite-end" className="block text-sm font-medium text-foreground mb-1.5">
                Pick-Fenster Ende
              </label>
              <DateTimeInput
                id="invite-end"
                value={inviteWindowEnd}
                onChange={(e) => setInviteWindowEnd(e.target.value)}
                className="w-full bg-background border border-border rounded-lg px-3 py-2 text-foreground focus:outline-none focus:ring-2 focus:ring-primary/50"
              />
            </div>
          </div>
        )}

        {/* Spielmodus + Auto-Lobby */}
        <div className="rounded-lg border border-border overflow-hidden">
          <div className="flex items-center gap-2 px-4 py-3 bg-surface border-b border-border">
            <Swords size={15} className="text-primary" />
            <span className="text-sm font-medium text-foreground">Spielmodus & Lobby-Automatik</span>
          </div>
          <div className="space-y-4 px-4 py-4">
            <div>
              <label htmlFor="tournament-game-mode" className="block text-sm font-medium text-foreground mb-1.5">
                Game-Modus für alle Matches
              </label>
              <select
                id="tournament-game-mode"
                value={tournamentGameMode}
                onChange={(event) => setTournamentGameMode(event.target.value as TournamentGameMode)}
                className="w-full bg-background border border-border rounded-lg px-3 py-2 text-foreground focus:outline-none focus:ring-2 focus:ring-primary/50"
              >
                {GAME_MODE_OPTIONS.map((option) => (
                  <option key={option.value} value={option.value}>{option.label}</option>
                ))}
              </select>
              <p className="mt-1 text-xs text-muted">
                {GAME_MODE_OPTIONS.find((option) => option.value === tournamentGameMode)?.description}
              </p>
            </div>

            <label className="flex items-start gap-3 cursor-pointer">
              <input
                type="checkbox"
                checked={autoLobbyEnabled}
                onChange={(event) => setAutoLobbyEnabled(event.target.checked)}
                className="mt-0.5 h-4 w-4 accent-primary"
              />
              <div>
                <div className="text-sm font-medium text-foreground">Lobbys automatisch erstellen</div>
                <div className="text-xs text-muted">
                  Sobald Bracket/Gruppen generiert sind oder ein Match-Ergebnis eingetragen wird, legt der
                  Bot die nächsten Lobbys ohne Klick an.
                </div>
              </div>
            </label>
          </div>
        </div>

        {/* Match-Modus Preset */}
        <div>
          <label htmlFor="lobby-preset" className="block text-sm font-medium text-foreground mb-1.5">
            Lobby-ConVars-Preset
          </label>
          <select
            id="lobby-preset"
            value={lobbyPreset}
            onChange={(e) => {
              setLobbyPreset(e.target.value as LobbySettingsPreset)
              setCustomJsonError('')
            }}
            className="w-full bg-background border border-border rounded-lg px-3 py-2 text-foreground focus:outline-none focus:ring-2 focus:ring-primary/50"
          >
            {(Object.keys(PRESET_LABELS) as LobbySettingsPreset[]).map((key) => (
              <option key={key} value={key}>{PRESET_LABELS[key]}</option>
            ))}
          </select>
          {hasActiveSliders && lobbyPreset !== 'custom' && (
            <p className="mt-1 text-xs text-muted">
              Aktive Regler werden mit den Preset-Werten zusammengeführt und als Custom gespeichert.
            </p>
          )}
        </div>

        {lobbyPreset === 'custom' && (
          <div>
            <label htmlFor="custom-json" className="block text-sm font-medium text-foreground mb-1.5">
              Custom Convars (JSON)
            </label>
            <textarea
              id="custom-json"
              value={customJson}
              onChange={(e) => { setCustomJson(e.target.value); setCustomJsonError('') }}
              rows={4}
              placeholder={'{"sv_gravity": 400, "citadel_dps_multiplier": 2}'}
              className="w-full bg-background border border-border rounded-lg px-3 py-2 text-foreground placeholder:text-muted font-mono text-sm focus:outline-none focus:ring-2 focus:ring-primary/50 resize-none"
            />
            {customJsonError && (
              <p className="mt-1 text-xs text-red-400">{customJsonError}</p>
            )}
          </div>
        )}

        {/* Regler */}
        <div className="border border-border rounded-lg overflow-hidden">
          <div className="flex items-center gap-2 px-4 py-3 bg-surface border-b border-border">
            <SlidersHorizontal size={15} className="text-primary" />
            <span className="text-sm font-medium text-foreground">Erweiterte Regler</span>
            <span className="ml-auto text-xs text-muted">überschreiben Preset-Werte</span>
          </div>
          <div className="divide-y divide-border">
            {CONVAR_SLIDERS.map((cfg) => {
              const state = sliders[cfg.key]
              return (
                <div key={cfg.key} className="px-4 py-3 space-y-2">
                  <div className="flex items-center justify-between">
                    <div className="flex items-center gap-2">
                      {/* Toggle */}
                      <button
                        type="button"
                        onClick={() => setSliderEnabled(cfg.key, !state.enabled)}
                        className={`relative w-9 h-5 rounded-full transition-colors ${
                          state.enabled ? 'bg-primary' : 'bg-border'
                        }`}
                      >
                        <span
                          className={`absolute top-0.5 left-0.5 w-4 h-4 bg-white rounded-full shadow transition-transform ${
                            state.enabled ? 'translate-x-4' : 'translate-x-0'
                          }`}
                        />
                      </button>
                      <span className={`text-sm font-medium ${state.enabled ? 'text-foreground' : 'text-muted'}`}>
                        {cfg.label}
                      </span>
                    </div>
                    {state.enabled && (
                      <span className="text-sm font-mono font-semibold text-primary">
                        {state.value}{cfg.unit}
                      </span>
                    )}
                  </div>

                  {state.enabled && (
                    <div className="pl-11 space-y-1">
                      <input
                        type="range"
                        min={cfg.min}
                        max={cfg.max}
                        step={cfg.step}
                        value={state.value}
                        onChange={(e) => setSliderValue(cfg.key, parseFloat(e.target.value))}
                        className="w-full accent-primary"
                      />
                      <div className="flex justify-between text-xs text-muted">
                        <span>{cfg.min}{cfg.unit}</span>
                        <span className="text-center text-muted/70">{cfg.hint}</span>
                        <span>{cfg.max}{cfg.unit}</span>
                      </div>
                    </div>
                  )}
                </div>
              )
            })}
          </div>
        </div>

        {/* Vorschau der aktiven Convars */}
        {hasActiveSliders && (
          <div className="bg-surface border border-border rounded-lg p-3">
            <p className="text-xs font-medium text-muted mb-1.5">Gesendete Convars (Vorschau)</p>
            <pre className="text-xs text-foreground font-mono overflow-x-auto">
              {JSON.stringify(
                buildFinalConvars(
                  lobbyPreset,
                  (() => { try { return JSON.parse(customJson) } catch { return undefined } })(),
                  sliders,
                ) ?? PRESET_BASE_CONVARS[lobbyPreset],
                null,
                2,
              )}
            </pre>
          </div>
        )}

        {/* Fehler */}
        {createMutation.isError && (
          <div className="flex items-center gap-2 text-red-400 text-sm bg-red-500/10 border border-red-500/20 rounded-lg p-3">
            <AlertCircle size={16} />
            <span>
              {createMutation.error instanceof Error
                ? createMutation.error.message
                : 'Fehler beim Erstellen'}
            </span>
          </div>
        )}

        {/* Erfolg */}
        {successMsg && (
          <div className="flex items-center gap-2 text-green-400 text-sm bg-green-500/10 border border-green-500/20 rounded-lg p-3">
            <CheckCircle size={16} />
            <span>{successMsg}</span>
          </div>
        )}

        <Button
          type="submit"
          variant="primary"
          size="lg"
          disabled={createMutation.isPending || !name.trim()}
          className="w-full"
        >
          {createMutation.isPending ? 'Wird erstellt...' : 'Turnier erstellen'}
        </Button>
      </form>
    </Card>
  )
}
