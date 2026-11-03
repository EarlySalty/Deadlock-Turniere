import type { LobbySettingsPreset, TournamentGameMode } from '@/types/tournament'
import Field from './Field'
import { SubHeading } from './SettingsSection'
import {
  CONVAR_SLIDERS,
  GAME_MODE_OPTIONS,
  INPUT_CLASS,
  MATCH_OBJECTIVE_OPTIONS,
  PRESET_LABELS,
  optionDescription,
  type ConvarSliderConfig,
} from './options'
import { hasActiveSliders, previewConvars, type GameRulesState, type SliderState } from './gameRules'

interface GameRulesFieldsProps {
  idPrefix: string
  value: GameRulesState
  onChange: (value: GameRulesState) => void
  league: boolean
  customJsonError?: string
}

const AUTO_LOBBY_HINT = {
  league:
    'Sobald der Termin einer Runde erreicht ist und die Runde davor gewertet ist, legt der Bot die Lobbys an. Aus: nur per Klick auf „Lobby erstellen“.',
  bracket:
    'Nach dem Erstellen von Gruppen oder Bracket und nach jedem Ergebnis legt der Bot die nächsten Lobbys an. Aus: nur per Klick auf „Lobby erstellen“.',
}

export default function GameRulesFields({
  idPrefix,
  value,
  onChange,
  league,
  customJsonError,
}: GameRulesFieldsProps) {
  const update = (patch: Partial<GameRulesState>) => onChange({ ...value, ...patch })
  const updateSlider = (key: string, patch: Partial<SliderState>) =>
    update({ sliders: { ...value.sliders, [key]: { ...value.sliders[key], ...patch } } })
  const slidersActive = hasActiveSliders(value.sliders)

  return (
    <div className="space-y-5">
      <div className="grid gap-4 md:grid-cols-2">
        <Field
          id={`${idPrefix}-game-mode`}
          label="Game-Modus"
          hint={optionDescription(GAME_MODE_OPTIONS, value.gameMode)}
        >
          <select
            id={`${idPrefix}-game-mode`}
            value={value.gameMode}
            onChange={(event) => update({ gameMode: event.target.value as TournamentGameMode })}
            className={INPUT_CLASS}
          >
            {GAME_MODE_OPTIONS.map((option) => (
              <option key={option.value} value={option.value}>{option.label}</option>
            ))}
          </select>
        </Field>

        <Field
          id={`${idPrefix}-match-objective`}
          label="Siegbedingung"
          hint="Wird den Spielern in der Lobby-Ansage genannt."
        >
          <select
            id={`${idPrefix}-match-objective`}
            value={value.matchObjective}
            onChange={(event) => update({ matchObjective: event.target.value })}
            className={INPUT_CLASS}
          >
            {MATCH_OBJECTIVE_OPTIONS.map((option) => (
              <option key={option.value} value={option.value}>{option.label}</option>
            ))}
          </select>
        </Field>
      </div>

      <label className="flex cursor-pointer items-start gap-3 rounded-lg border border-border px-3 py-2.5">
        <input
          type="checkbox"
          checked={value.autoLobby}
          onChange={(event) => update({ autoLobby: event.target.checked })}
          className="mt-0.5 h-4 w-4 accent-primary"
        />
        <span>
          <span className="block text-sm font-medium text-foreground">Lobbys automatisch erstellen</span>
          <span className="block text-xs text-muted">{league ? AUTO_LOBBY_HINT.league : AUTO_LOBBY_HINT.bracket}</span>
        </span>
      </label>

      <div className="space-y-3">
        <SubHeading>Lobby-Einstellungen</SubHeading>
        <Field
          id={`${idPrefix}-lobby-preset`}
          label="Lobby-ConVars-Preset"
          hint={
            slidersActive && value.preset !== 'custom'
              ? 'Eingeschaltete Regler werden mit diesem Preset zusammengeführt und als eigene Werte gespeichert.'
              : 'Fertige Spielregeln für jede Lobby, z. B. schnellere Cooldowns oder weniger Schwerkraft.'
          }
        >
          <select
            id={`${idPrefix}-lobby-preset`}
            value={value.preset}
            onChange={(event) => update({ preset: event.target.value as LobbySettingsPreset })}
            className={INPUT_CLASS}
          >
            {(Object.keys(PRESET_LABELS) as LobbySettingsPreset[]).map((key) => (
              <option key={key} value={key}>{PRESET_LABELS[key]}</option>
            ))}
          </select>
        </Field>

        {value.preset === 'custom' && (
          <Field
            id={`${idPrefix}-custom-json`}
            label="Eigene Werte (JSON)"
            hint={customJsonError ? <span className="text-red-400">{customJsonError}</span> : undefined}
          >
            <textarea
              id={`${idPrefix}-custom-json`}
              value={value.customJson}
              onChange={(event) => update({ customJson: event.target.value })}
              rows={4}
              placeholder={'{"sv_gravity": 400, "citadel_dps_multiplier": 2}'}
              className={`${INPUT_CLASS} resize-y font-mono text-sm`}
            />
          </Field>
        )}
      </div>

      <div className="overflow-hidden rounded-lg border border-border">
        <div className="border-b border-border bg-surface px-4 py-3">
          <div className="text-sm font-medium text-foreground">Erweiterte Regler</div>
          <p className="text-xs text-muted">
            Optional. Schalte einen Regler ein, um genau diesen Wert zu ändern. Er überschreibt den Wert aus dem Preset.
          </p>
        </div>
        <div className="divide-y divide-border">
          {CONVAR_SLIDERS.map((config) => (
            <SliderRow
              key={config.key}
              config={config}
              state={value.sliders[config.key]}
              onChange={(patch) => updateSlider(config.key, patch)}
            />
          ))}
        </div>
      </div>

      {slidersActive && (
        <div className="rounded-lg border border-border bg-surface p-3">
          <p className="mb-1.5 text-xs font-medium text-muted">Diese Werte bekommt jede Lobby</p>
          <pre className="overflow-x-auto font-mono text-xs text-foreground">
            {JSON.stringify(previewConvars(value), null, 2)}
          </pre>
        </div>
      )}
    </div>
  )
}

interface SliderRowProps {
  config: ConvarSliderConfig
  state: SliderState
  onChange: (patch: Partial<SliderState>) => void
}

function SliderRow({ config, state, onChange }: SliderRowProps) {
  return (
    <div className="space-y-2 px-4 py-3">
      <div className="flex items-center justify-between gap-3">
        <label className="flex cursor-pointer items-center gap-3">
          <button
            type="button"
            role="switch"
            aria-checked={state.enabled}
            aria-label={config.label}
            onClick={() => onChange({ enabled: !state.enabled })}
            className={`relative h-5 w-9 shrink-0 rounded-full transition-colors ${state.enabled ? 'bg-primary' : 'bg-border'}`}
          >
            <span
              className={`absolute left-0.5 top-0.5 h-4 w-4 rounded-full bg-white shadow transition-transform ${state.enabled ? 'translate-x-4' : 'translate-x-0'}`}
            />
          </button>
          <span className={`text-sm font-medium ${state.enabled ? 'text-foreground' : 'text-muted'}`}>
            {config.label}
          </span>
        </label>
        {state.enabled && (
          <span className="font-mono text-sm font-semibold text-primary">
            {state.value}{config.unit}
          </span>
        )}
      </div>
      {state.enabled && (
        <div className="space-y-1 sm:pl-12">
          <input
            type="range"
            aria-label={`${config.label} Wert`}
            min={config.min}
            max={config.max}
            step={config.step}
            value={state.value}
            onChange={(event) => onChange({ value: parseFloat(event.target.value) })}
            className="w-full accent-primary"
          />
          <div className="flex justify-between text-xs text-muted">
            <span>{config.min}{config.unit}</span>
            <span>{config.max}{config.unit}</span>
          </div>
          <p className="text-xs text-muted">{config.hint}</p>
        </div>
      )}
    </div>
  )
}
