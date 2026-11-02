import { useState } from 'react'
import type { FormEvent } from 'react'
import Card from '@/components/ui/Card'
import Button from '@/components/ui/Button'
import { useCreateTournament } from '@/hooks/useTournament'
import { localInputToIso } from '@/lib/league'
import { AlertCircle, CheckCircle, Copy, Trophy } from 'lucide-react'
import ChoiceCards from './settings/ChoiceCards'
import GameRulesFields from './settings/GameRulesFields'
import SpecialSettingsFields from './settings/SpecialSettingsFields'
import { CollapsibleSection, SettingsSection, SubHeading } from './settings/SettingsSection'
import { resolveLobbyPayload, type GameRulesState } from './settings/gameRules'
import {
  defaultCreateValues,
  type CreateFormValues,
  type CreateTemplate,
} from './settings/createFormValues'
import {
  BasicsFields,
  DateFields,
  MatchFormatFields,
  ReminderFields,
  type DateEntry,
} from './settings/StandardFields'
import {
  DATE_HINTS,
  DEFAULT_REMINDER_OFFSETS,
  FORMAT_OPTIONS,
  GAME_MODE_OPTIONS,
  PRESET_LABELS,
  parseOffsets,
} from './settings/options'

interface CreateTournamentFormProps {
  template?: CreateTemplate
}

interface CreateFormDates {
  regStart: string
  regEnd: string
  checkinStart: string
  leagueStart: string
}

const EMPTY_DATES: CreateFormDates = { regStart: '', regEnd: '', checkinStart: '', leagueStart: '' }

function dateEntries(
  dates: CreateFormDates,
  updateDates: (patch: Partial<CreateFormDates>) => void,
  league: boolean,
): DateEntry[] {
  const entries: DateEntry[] = [
    { key: 'reg-start', label: 'Anmeldung Start', value: dates.regStart, onChange: (regStart) => updateDates({ regStart }) },
    { key: 'reg-end', label: 'Turnier-Start', hint: DATE_HINTS.registrationEnd, value: dates.regEnd, onChange: (regEnd) => updateDates({ regEnd }) },
    { key: 'checkin-start', label: 'Check-in Start', hint: DATE_HINTS.checkinStart, value: dates.checkinStart, onChange: (checkinStart) => updateDates({ checkinStart }) },
  ]
  if (!league) return entries
  return [
    ...entries,
    {
      key: 'league-first-round',
      label: 'Erste Runde beginnt',
      hint: 'Der Spielplan entsteht beim Abschluss des Check-ins, jede Runde dauert zunächst 50 Minuten. Start und Rundenlänge änderst du später unter „Spielplan“.',
      value: dates.leagueStart,
      onChange: (leagueStart) => updateDates({ leagueStart }),
    },
  ]
}

function gameRulesSummary(rules: GameRulesState): string {
  const mode = GAME_MODE_OPTIONS.find((option) => option.value === rules.gameMode)?.label ?? rules.gameMode
  const lobby = rules.autoLobby ? 'Lobbys automatisch' : 'Lobbys per Klick'
  return `Aktuell: ${mode} · Preset ${PRESET_LABELS[rules.preset]} · ${lobby}`
}

export default function CreateTournamentForm({ template }: CreateTournamentFormProps) {
  const [values, setValues] = useState<CreateFormValues>(() => template?.values ?? defaultCreateValues())
  const [dates, setDates] = useState<CreateFormDates>(EMPTY_DATES)
  const [rulesOpen, setRulesOpen] = useState(false)
  const [customJsonError, setCustomJsonError] = useState('')
  const [successMsg, setSuccessMsg] = useState('')
  const createMutation = useCreateTournament()
  const league = values.tournamentFormat === 'league'

  const update = (patch: Partial<CreateFormValues>) => setValues((current) => ({ ...current, ...patch }))
  const updateDates = (patch: Partial<CreateFormDates>) => setDates((current) => ({ ...current, ...patch }))

  const handleSubmit = (event: FormEvent) => {
    event.preventDefault()
    setSuccessMsg('')
    const lobby = resolveLobbyPayload(values.gameRules)
    if (!lobby.ok) {
      setCustomJsonError(lobby.error)
      setRulesOpen(true)
      return
    }
    setCustomJsonError('')
    const { special, gameRules } = values
    createMutation.mutate(
      {
        name: values.name.trim(),
        description: values.description.trim() || undefined,
        team_size: values.teamSize,
        bracket_format: values.bracketFormat,
        force_tournament_mode: values.tournamentFormat === 'auto' ? undefined : values.tournamentFormat,
        group_phase_start: league ? localInputToIso(dates.leagueStart) : undefined,
        series_format: league ? undefined : values.seriesFormat,
        final_series_format: league ? undefined : values.finalSeriesFormat ?? undefined,
        registration_start: dates.regStart || undefined,
        registration_end: dates.regEnd || undefined,
        checkin_start: dates.checkinStart || undefined,
        invite_mode: special.inviteMode,
        invite_window_start: special.inviteMode === 'window' ? special.inviteWindowStart || undefined : undefined,
        invite_window_end: special.inviteMode === 'window' ? special.inviteWindowEnd || undefined : undefined,
        lobby_settings_preset: lobby.preset,
        lobby_settings: lobby.settings,
        tournament_game_mode: gameRules.gameMode,
        auto_lobby_enabled: gameRules.autoLobby,
        exclude_from_leaderboard: special.excludeFromLeaderboard,
        reminder_offsets: parseOffsets(values.reminderOffsets, DEFAULT_REMINDER_OFFSETS),
        start_reminder_offsets: parseOffsets(values.startReminderOffsets, DEFAULT_REMINDER_OFFSETS),
        match_objective: gameRules.matchObjective,
        no_show_grace_minutes: special.noShowGrace,
        rules: values.rules.trim() || undefined,
      },
      {
        onSuccess: (tournament) => {
          setSuccessMsg(`Turnier "${tournament.name}" wurde erfolgreich erstellt!`)
          setValues(defaultCreateValues())
          setDates(EMPTY_DATES)
        },
      },
    )
  }

  return (
    <Card className="p-4 sm:p-6">
      <div className="mb-5 flex items-center gap-2">
        <Trophy size={20} className="text-primary" />
        <h2 className="text-lg font-semibold text-foreground">Neues Turnier erstellen</h2>
      </div>

      {template && (
        <div className="mb-5 flex items-start gap-2 rounded-lg border border-primary/30 bg-primary/5 p-3 text-sm text-foreground">
          <Copy size={16} className="mt-0.5 shrink-0 text-primary" />
          <span>
            Einstellungen aus „{template.sourceName}“ übernommen. Teams und Anmeldungen werden nicht kopiert,
            die Termine setzt du neu.
          </span>
        </div>
      )}

      <form onSubmit={handleSubmit} className="space-y-5">
        <SettingsSection
          title="Standard-Einstellungen"
          description="Das Wichtigste für jedes Turnier: Name, Ablauf, Termine und Erinnerungen."
        >
          <BasicsFields idPrefix="create" value={values} onChange={update} required />
          <div className="space-y-4">
            <SubHeading>Ablauf</SubHeading>
            <ChoiceCards
              name="create-tournament-format"
              legend="Turniermodus"
              options={FORMAT_OPTIONS}
              value={values.tournamentFormat}
              onChange={(tournamentFormat) => update({ tournamentFormat })}
            />
            <MatchFormatFields idPrefix="create" value={values} onChange={update} league={league} />
          </div>
          <DateFields idPrefix="create" entries={dateEntries(dates, updateDates, league)} />
          <ReminderFields idPrefix="create" value={values} onChange={update} />
        </SettingsSection>

        <SettingsSection
          title="Spezielle Einstellungen"
          description="Nur ändern, wenn das Turnier von den üblichen Regeln abweicht."
        >
          <SpecialSettingsFields
            idPrefix="create"
            value={values.special}
            onChange={(special) => update({ special })}
          />
        </SettingsSection>

        <CollapsibleSection
          title="Spielregeln"
          description="Was in jeder Lobby gilt. Die Voreinstellungen passen für ein normales Turnier und lassen sich später ändern."
          summary={gameRulesSummary(values.gameRules)}
          open={rulesOpen}
          onToggle={setRulesOpen}
        >
          <GameRulesFields
            idPrefix="create"
            value={values.gameRules}
            onChange={(gameRules) => {
              update({ gameRules })
              setCustomJsonError('')
            }}
            league={league}
            customJsonError={customJsonError}
          />
        </CollapsibleSection>

        {createMutation.isError && (
          <div className="flex items-center gap-2 rounded-lg border border-red-500/20 bg-red-500/10 p-3 text-sm text-red-400">
            <AlertCircle size={16} />
            <span>
              {createMutation.error instanceof Error ? createMutation.error.message : 'Fehler beim Erstellen'}
            </span>
          </div>
        )}

        {customJsonError && (
          <div className="flex items-center gap-2 rounded-lg border border-red-500/20 bg-red-500/10 p-3 text-sm text-red-400">
            <AlertCircle size={16} />
            <span>Spielregeln: {customJsonError}</span>
          </div>
        )}

        {successMsg && (
          <div className="flex items-center gap-2 rounded-lg border border-green-500/20 bg-green-500/10 p-3 text-sm text-green-400">
            <CheckCircle size={16} />
            <span>{successMsg}</span>
          </div>
        )}

        <Button
          type="submit"
          variant="primary"
          size="lg"
          disabled={createMutation.isPending || !values.name.trim()}
          className="w-full"
        >
          {createMutation.isPending ? 'Wird erstellt...' : 'Turnier erstellen'}
        </Button>
      </form>
    </Card>
  )
}
