import { useState } from 'react'
import Card from '@/components/ui/Card'
import Badge from '@/components/ui/Badge'
import Button from '@/components/ui/Button'
import DateTimeInput from '@/components/ui/DateTimeInput'
import {
  useAdvanceTournament,
  useAssignRandomTeams,
  useDeleteTournament,
  useGenerateBracket,
  useGenerateGroups,
  useOpenCheckin,
  useRevertCheckin,
  useUpdateTournament,
} from '@/hooks/useTournament'
import type {
  Tournament,
  TournamentGameMode,
  TournamentMode,
  TournamentUpdate,
} from '@/types/tournament'
import {
  AlertCircle,
  ArrowRight,
  CalendarRange,
  CheckCircle2,
  GitBranch,
  PencilLine,
  Shuffle,
  Swords,
  Trash2,
  Trophy,
  Users,
} from 'lucide-react'

const GAME_MODE_LABELS: Record<TournamentGameMode, string> = {
  standard: 'Standard',
  mirror: 'Mirror Match',
  all_same: 'All Same Hero',
  random_heroes: 'Random Heroes',
  single_lane: 'Single Lane Battle',
}

interface TournamentManagerProps {
  tournament: Tournament
  teamCount: number
  playerCount: number
  matchCount: number
  canChangeTournamentMode: boolean
}

const STATUS_ACTIONS: Record<string, { label: string; confirmMsg: string }> = {
  draft: { label: 'Anmeldung starten', confirmMsg: 'Turnier-Anmeldung wirklich starten?' },
  group_phase: { label: 'Zum Bracket', confirmMsg: 'Gruppenphase beenden und Bracket starten?' },
  bracket: { label: 'Turnier abschliessen', confirmMsg: 'Turnier wirklich abschliessen?' },
  completed: { label: 'Archivieren', confirmMsg: 'Turnier ins Archiv verschieben?' },
}

function toInputDateTime(value: string | null | undefined): string {
  if (!value) return ''
  return value.replace(' ', 'T').slice(0, 16)
}

function toReminderOffsetString(value: number[] | null | undefined): string {
  return (value && value.length > 0 ? value : [1440, 120, 15]).join(', ')
}

function parseReminderOffsets(value: string): number[] {
  const parsed = value
    .split(',')
    .map((entry) => Number(entry.trim()))
    .filter((entry) => Number.isFinite(entry) && entry >= 0)
  return parsed.length > 0 ? parsed : [1440, 120, 15]
}

export default function TournamentManager({
  tournament,
  teamCount,
  playerCount,
  matchCount,
  canChangeTournamentMode,
}: TournamentManagerProps) {
  const detailsUpdateMutation = useUpdateTournament()
  const advanceMutation = useAdvanceTournament()
  const openCheckinMutation = useOpenCheckin(tournament.id)
  const revertCheckinMutation = useRevertCheckin(tournament.id)
  const assignMutation = useAssignRandomTeams()
  const deleteMutation = useDeleteTournament()
  const generateGroupsMutation = useGenerateGroups()
  const generateBracketMutation = useGenerateBracket()

  const [form, setForm] = useState({
    name: tournament.name,
    description: tournament.description ?? '',
    team_size: tournament.team_size,
    series_format: tournament.series_format as 1 | 3 | 5,
    final_series_format: tournament.final_series_format as 1 | 3 | 5 | null,
    bracket_format: tournament.bracket_format,
    tournament_mode: tournament.tournament_mode,
    tournament_game_mode: tournament.tournament_game_mode,
    auto_lobby_enabled: tournament.auto_lobby_enabled,
    registration_start: toInputDateTime(tournament.registration_start),
    registration_end: toInputDateTime(tournament.registration_end),
    checkin_start: toInputDateTime(tournament.checkin_start ?? null),
    group_phase_start: toInputDateTime(tournament.group_phase_start),
    bracket_start: toInputDateTime(tournament.bracket_start),
    exclude_from_leaderboard: tournament.exclude_from_leaderboard,
    reminder_offsets: toReminderOffsetString(tournament.reminder_offsets),
    rules: tournament.rules ?? '',
  })
  const [successMessage, setSuccessMessage] = useState('')
  const [deleteConfirmed, setDeleteConfirmed] = useState(false)

  // Admin keys this component by tournament ID and revision; remount resets confirmation.

  const action = STATUS_ACTIONS[tournament.status]
  const isLoading =
    detailsUpdateMutation.isPending ||
    advanceMutation.isPending ||
    openCheckinMutation.isPending ||
    revertCheckinMutation.isPending ||
    assignMutation.isPending ||
    deleteMutation.isPending ||
    generateGroupsMutation.isPending ||
    generateBracketMutation.isPending

  const error =
    detailsUpdateMutation.error ||
    advanceMutation.error ||
    openCheckinMutation.error ||
    revertCheckinMutation.error ||
    assignMutation.error ||
    deleteMutation.error ||
    generateGroupsMutation.error ||
    generateBracketMutation.error

  const handleChange = (
    key: keyof typeof form,
    value: string | number | boolean | null
  ) => {
    setForm((current) => ({ ...current, [key]: value }))
  }

  const handleSave = () => {
    const payload: TournamentUpdate = {
      name: form.name.trim(),
      description: form.description.trim() || undefined,
      team_size: form.team_size,
      series_format: form.series_format as 1 | 3 | 5,
      final_series_format: (form.final_series_format as 1 | 3 | 5 | null) ?? null,
      bracket_format: form.bracket_format,
      tournament_game_mode: form.tournament_game_mode,
      auto_lobby_enabled: form.auto_lobby_enabled,
      registration_start: form.registration_start || undefined,
      registration_end: form.registration_end || undefined,
      checkin_start: form.checkin_start || undefined,
      group_phase_start: form.group_phase_start || undefined,
      bracket_start: form.bracket_start || undefined,
      exclude_from_leaderboard: form.exclude_from_leaderboard,
      reminder_offsets: parseReminderOffsets(form.reminder_offsets),
      rules: form.rules.trim() || null,
    }
    if (canChangeTournamentMode && form.tournament_mode !== tournament.tournament_mode) {
      payload.force_tournament_mode = form.tournament_mode as TournamentMode
    }
    detailsUpdateMutation.mutate(
      { id: tournament.id, data: payload },
      { onSuccess: () => setSuccessMessage('Turnierdaten gespeichert.') }
    )
  }

  const handleAdvance = () => {
    if (!action) return
    if (window.confirm(action.confirmMsg)) {
      advanceMutation.mutate(tournament.id, {
        onSuccess: () => setSuccessMessage(`Status auf ${action.label} verarbeitet.`),
      })
    }
  }

  const handleAssignRandom = () => {
    if (window.confirm('Solo-Spieler zufällig auf Teams verteilen?')) {
      assignMutation.mutate(tournament.id, {
        onSuccess: () => setSuccessMessage('Solo-Anmeldungen wurden Teams zugewiesen.'),
      })
    }
  }

  const handleOpenCheckin = () => {
    if (window.confirm('Check-in jetzt öffnen?')) {
      openCheckinMutation.mutate(undefined, {
        onSuccess: () => setSuccessMessage('Check-in wurde geöffnet.'),
      })
    }
  }

  const handleRevertCheckin = () => {
    if (window.confirm('Check-in zurück auf Anmeldung setzen und alle Check-ins löschen?')) {
      revertCheckinMutation.mutate(undefined, {
        onSuccess: () => setSuccessMessage('Check-in wurde zurückgesetzt. Das Turnier ist wieder in der Anmeldung.'),
      })
    }
  }

  const handleGenerateGroups = () => {
    generateGroupsMutation.mutate(
      { tournamentId: tournament.id, numGroups: 4 },
      { onSuccess: () => setSuccessMessage('Gruppen und Gruppen-Matches wurden generiert.') }
    )
  }

  const handleGenerateBracket = () => {
    generateBracketMutation.mutate(tournament.id, {
      onSuccess: () => setSuccessMessage('Bracket wurde neu generiert.'),
    })
  }

  const handleDelete = () => {
    if (!deleteConfirmed) {
      if (window.confirm(`Turnier "${tournament.name}" unwiderruflich löschen?\n\nDiese Aktion kann nicht rückgängig gemacht werden!`)) {
        setDeleteConfirmed(true)
      }
    } else {
      deleteMutation.mutate(tournament.id, {
        onSuccess: () => {
          setDeleteConfirmed(false)
          setSuccessMessage('Turnier wurde gelöscht.')
        },
        onError: () => {
          setDeleteConfirmed(false)
        }
      })
    }
  }

  return (
    <Card className="p-6 space-y-6">
      <div className="flex flex-col gap-4 lg:flex-row lg:items-start lg:justify-between">
        <div className="space-y-2">
          <div className="flex items-center gap-3">
            <Trophy size={20} className="text-primary" />
            <h2 className="text-lg font-semibold text-foreground">Turnier-Verwaltung</h2>
            <Badge status={tournament.status} />
          </div>
          <p className="text-sm text-muted">
            Metadaten pflegen, Phasen steuern und Generierung sicher auslösen.
          </p>
        </div>

        <div className="grid grid-cols-2 gap-3 sm:grid-cols-4">
          <Card className="p-3 text-center">
            <div className="text-2xl font-bold text-primary">{tournament.team_size}</div>
            <div className="text-xs text-muted">Teamgröße</div>
          </Card>
          <Card className="p-3 text-center">
            <div className="text-2xl font-bold text-primary">{teamCount}</div>
            <div className="text-xs text-muted">Teams</div>
          </Card>
          <Card className="p-3 text-center">
            <div className="text-2xl font-bold text-primary">{playerCount}</div>
            <div className="text-xs text-muted">Spieler</div>
          </Card>
          <Card className="p-3 text-center">
            <div className="text-2xl font-bold text-primary">{matchCount}</div>
            <div className="text-xs text-muted">Matches</div>
          </Card>
        </div>
      </div>

      <div className="grid gap-4 lg:grid-cols-2">
        <div className="space-y-4">
          <div>
            <label htmlFor="admin-tournament-name" className="mb-1.5 block text-sm font-medium text-foreground">
              Turniername
            </label>
            <input
              id="admin-tournament-name"
              type="text"
              value={form.name}
              onChange={(event) => handleChange('name', event.target.value)}
              className="w-full rounded-lg border border-border bg-background px-3 py-2 text-foreground focus:outline-none focus:ring-2 focus:ring-primary/50"
            />
          </div>

          <div>
            <label htmlFor="admin-tournament-description" className="mb-1.5 block text-sm font-medium text-foreground">
              Beschreibung
            </label>
            <textarea
              id="admin-tournament-description"
              rows={4}
              value={form.description}
              onChange={(event) => handleChange('description', event.target.value)}
              className="w-full rounded-lg border border-border bg-background px-3 py-2 text-foreground focus:outline-none focus:ring-2 focus:ring-primary/50"
            />
          </div>

          <div>
            <label htmlFor="admin-tournament-rules" className="mb-1.5 block text-sm font-medium text-foreground">
              Regelwerk (Markdown unterstützt)
            </label>
            <textarea
              id="admin-tournament-rules"
              rows={10}
              value={form.rules}
              onChange={(event) => handleChange('rules', event.target.value)}
              className="w-full rounded-lg border border-border bg-background px-3 py-2 text-foreground focus:outline-none focus:ring-2 focus:ring-primary/50"
            />
          </div>
        </div>

        <div className="grid gap-4 sm:grid-cols-2">
          <div>
            <label htmlFor="admin-team-size" className="mb-1.5 block text-sm font-medium text-foreground">
              Teamgröße
            </label>
            <input
              id="admin-team-size"
              type="number"
              min={1}
              max={20}
              value={form.team_size}
              onChange={(event) => handleChange('team_size', Number(event.target.value))}
              className="w-full rounded-lg border border-border bg-background px-3 py-2 text-foreground focus:outline-none focus:ring-2 focus:ring-primary/50"
            />
          </div>

          <div>
            <label htmlFor="admin-bracket-format" className="mb-1.5 block text-sm font-medium text-foreground">
              Bracket-Format
            </label>
            <select
              id="admin-bracket-format"
              value={form.bracket_format}
              onChange={(event) => handleChange('bracket_format', event.target.value)}
              className="w-full rounded-lg border border-border bg-background px-3 py-2 text-foreground focus:outline-none focus:ring-2 focus:ring-primary/50"
            >
              <option value="single_elimination">Single Elimination</option>
              <option value="double_elimination">Double Elimination</option>
            </select>
          </div>

          <div>
            <label htmlFor="admin-series-format" className="mb-1.5 block text-sm font-medium text-foreground">
              Serienformat (Standard)
            </label>
            <select
              id="admin-series-format"
              value={form.series_format}
              onChange={(event) => handleChange('series_format', Number(event.target.value) as 1 | 3 | 5)}
              className="w-full rounded-lg border border-border bg-background px-3 py-2 text-foreground focus:outline-none focus:ring-2 focus:ring-primary/50"
            >
              <option value={1}>Bo1</option>
              <option value={3}>Bo3</option>
              <option value={5}>Bo5</option>
            </select>
          </div>

          <div>
            <label htmlFor="admin-final-series-format" className="mb-1.5 block text-sm font-medium text-foreground">
              Finale Match Format
            </label>
            <select
              id="admin-final-series-format"
              value={form.final_series_format ?? 'same'}
              onChange={(event) => {
                const v = event.target.value
                handleChange('final_series_format', v === 'same' ? null : Number(v))
              }}
              className="w-full rounded-lg border border-border bg-background px-3 py-2 text-foreground focus:outline-none focus:ring-2 focus:ring-primary/50"
            >
              <option value="same">Wie Serienformat</option>
              <option value={1}>Bo1</option>
              <option value={3}>Bo3</option>
              <option value={5}>Bo5</option>
            </select>
            <p className="mt-1 text-xs text-muted">Überschreibt das Format nur für das Finale.</p>
          </div>

          <div>
            <label htmlFor="admin-tournament-mode" className="mb-1.5 block text-sm font-medium text-foreground">
              Turniermodus
            </label>
            <select
              id="admin-tournament-mode"
              value={form.tournament_mode}
              onChange={(event) => handleChange('tournament_mode', event.target.value)}
              disabled={!canChangeTournamentMode}
              className="w-full rounded-lg border border-border bg-background px-3 py-2 text-foreground focus:outline-none focus:ring-2 focus:ring-primary/50 disabled:opacity-60"
            >
              <option value="bracket_only">Nur Bracket</option>
              <option value="group_stage">Gruppenphase + Bracket</option>
            </select>
            <p className="mt-1 text-xs text-muted">
              Änderbar in Check-in oder in der Gruppenphase, solange noch kein Gruppenmatch gespielt wurde.
            </p>
          </div>

          <div>
            <label htmlFor="admin-game-mode" className="mb-1.5 block text-sm font-medium text-foreground">
              <Swords size={14} className="mr-1 inline text-primary" />
              Game-Modus
            </label>
            <select
              id="admin-game-mode"
              value={form.tournament_game_mode}
              onChange={(event) =>
                handleChange('tournament_game_mode', event.target.value as TournamentGameMode)
              }
              className="w-full rounded-lg border border-border bg-background px-3 py-2 text-foreground focus:outline-none focus:ring-2 focus:ring-primary/50"
            >
              {(Object.keys(GAME_MODE_LABELS) as TournamentGameMode[]).map((mode) => (
                <option key={mode} value={mode}>
                  {GAME_MODE_LABELS[mode]}
                </option>
              ))}
            </select>
            <p className="mt-1 text-xs text-muted">
              Hero-Auswahl-Logik (Mirror, AllSame, Random) bzw. Single-Lane-Battle. Änderungen wirken
              auf alle ab jetzt erstellten Lobbys.
            </p>
          </div>

          <label className="flex items-start gap-3 rounded-lg border border-border px-3 py-2 cursor-pointer sm:col-span-2">
            <input
              type="checkbox"
              checked={form.auto_lobby_enabled}
              onChange={(event) => handleChange('auto_lobby_enabled', event.target.checked)}
              className="mt-0.5 h-4 w-4 accent-primary"
            />
            <div>
              <div className="text-sm font-medium text-foreground">Lobbys automatisch erstellen</div>
              <div className="text-xs text-muted">
                Wenn an: Bot legt Lobbys nach Bracket-/Gruppen-Generierung und nach jedem Match-Ergebnis
                automatisch an. Aus = nur manuell per „Lobby erstellen".
              </div>
            </div>
          </label>

          <div>
            <label htmlFor="admin-reg-start" className="mb-1.5 block text-sm font-medium text-foreground">
              Anmeldung Start
            </label>
            <DateTimeInput
              id="admin-reg-start"
              value={form.registration_start}
              onChange={(event) => handleChange('registration_start', event.target.value)}
              className="w-full rounded-lg border border-border bg-background px-3 py-2 text-foreground focus:outline-none focus:ring-2 focus:ring-primary/50"
            />
          </div>

          <div>
            <label htmlFor="admin-reg-end" className="mb-1.5 block text-sm font-medium text-foreground">
              Turnier-Start
            </label>
            <DateTimeInput
              id="admin-reg-end"
              value={form.registration_end}
              onChange={(event) => handleChange('registration_end', event.target.value)}
              className="w-full rounded-lg border border-border bg-background px-3 py-2 text-foreground focus:outline-none focus:ring-2 focus:ring-primary/50"
            />
            <p className="mt-1 text-xs text-muted">
              Anmeldeschluss = Turnier-Start. Reminder werden vor diesem Zeitpunkt verschickt.
            </p>
          </div>

          <div>
            <label htmlFor="form-checkin-start" className="mb-1.5 block text-sm font-medium text-foreground">
              Check-in Start
            </label>
            <DateTimeInput
              id="form-checkin-start"
              value={form.checkin_start}
              onChange={(event) => handleChange('checkin_start', event.target.value)}
              className="w-full rounded-lg border border-border bg-background px-3 py-2 text-foreground focus:outline-none focus:ring-2 focus:ring-primary/50"
            />
            <p className="mt-1 text-xs text-muted">
              Falls leer: Check-in startet automatisch mit Turnier-Start.
            </p>
          </div>

          <div>
            <label htmlFor="admin-group-start" className="mb-1.5 block text-sm font-medium text-foreground">
              Gruppenphase Start
            </label>
            <DateTimeInput
              id="admin-group-start"
              value={form.group_phase_start}
              onChange={(event) => handleChange('group_phase_start', event.target.value)}
              className="w-full rounded-lg border border-border bg-background px-3 py-2 text-foreground focus:outline-none focus:ring-2 focus:ring-primary/50"
            />
          </div>

          <div>
            <label htmlFor="admin-reminder-offsets" className="mb-1.5 block text-sm font-medium text-foreground">
              Reminder vor Turnier-Start
            </label>
            <input
              id="admin-reminder-offsets"
              type="text"
              value={form.reminder_offsets}
              onChange={(event) => handleChange('reminder_offsets', event.target.value)}
              className="w-full rounded-lg border border-border bg-background px-3 py-2 text-foreground focus:outline-none focus:ring-2 focus:ring-primary/50"
            />
          </div>

          <label className="flex items-center gap-3 rounded-lg border border-border px-3 py-2 cursor-pointer sm:col-span-2">
            <input
              type="checkbox"
              checked={form.exclude_from_leaderboard}
              onChange={(event) => handleChange('exclude_from_leaderboard', event.target.checked)}
              className="h-4 w-4 accent-primary"
            />
            <div>
              <div className="text-sm font-medium text-foreground">Von Rangliste ausschließen</div>
              <div className="text-xs text-muted">Für Testturniere oder Events ohne Punktevergabe.</div>
            </div>
          </label>

          <div>
            <label htmlFor="admin-bracket-start" className="mb-1.5 block text-sm font-medium text-foreground">
              Bracket Start
            </label>
            <DateTimeInput
              id="admin-bracket-start"
              value={form.bracket_start}
              onChange={(event) => handleChange('bracket_start', event.target.value)}
              className="w-full rounded-lg border border-border bg-background px-3 py-2 text-foreground focus:outline-none focus:ring-2 focus:ring-primary/50"
            />
          </div>
        </div>
      </div>

      <div className="flex flex-wrap gap-2">
        <Button
          variant="primary"
          size="sm"
          disabled={isLoading || !form.name.trim()}
          onClick={handleSave}
        >
          <PencilLine size={14} />
          {detailsUpdateMutation.isPending ? 'Speichert...' : 'Änderungen speichern'}
        </Button>

        {tournament.status === 'registration' && (
          <Button variant="secondary" size="sm" disabled={isLoading} onClick={handleAssignRandom}>
            <Shuffle size={14} />
            {assignMutation.isPending ? 'Weist zu...' : 'Solo-Spieler zuweisen'}
          </Button>
        )}

        {tournament.status === 'group_phase' && (
          <Button variant="secondary" size="sm" disabled={isLoading} onClick={handleGenerateGroups}>
            <Users size={14} />
            {generateGroupsMutation.isPending ? 'Generiert...' : 'Gruppen neu generieren'}
          </Button>
        )}

        {tournament.status === 'bracket' && (
          <Button variant="secondary" size="sm" disabled={isLoading} onClick={handleGenerateBracket}>
            <GitBranch size={14} />
            {generateBracketMutation.isPending ? 'Generiert...' : 'Bracket neu generieren'}
          </Button>
        )}

        {tournament.status === 'registration' && (
          <Button variant="primary" size="sm" disabled={isLoading} onClick={handleOpenCheckin}>
            <ArrowRight size={14} />
            {openCheckinMutation.isPending ? 'Öffnet...' : 'Check-in öffnen'}
          </Button>
        )}

        {tournament.status === 'checkin' && (
          <Button variant="danger" size="sm" disabled={isLoading} onClick={handleRevertCheckin}>
            <ArrowRight size={14} />
            {revertCheckinMutation.isPending ? 'Setzt zurück...' : 'Check-in zurücksetzen'}
          </Button>
        )}

        {action && tournament.status !== 'archived' && (
          <Button variant="primary" size="sm" disabled={isLoading} onClick={handleAdvance}>
            <ArrowRight size={14} />
            {advanceMutation.isPending ? 'Verarbeitet...' : action.label}
          </Button>
        )}

        <Button
          variant={deleteConfirmed ? 'danger' : 'ghost'}
          size="sm"
          disabled={isLoading}
          onClick={handleDelete}
          className={deleteConfirmed ? 'border-red-500 bg-red-500/10 text-red-400 hover:bg-red-500/20' : ''}
        >
          <Trash2 size={14} />
          {deleteMutation.isPending
            ? 'Löscht...'
            : deleteConfirmed
              ? 'Wirklich löschen? (Letzte Bestätigung)'
              : 'Turnier löschen'}
        </Button>
      </div>

      {successMessage && (
        <div className="flex items-center gap-2 rounded-lg border border-green-500/20 bg-green-500/10 p-3 text-sm text-green-400">
          <CheckCircle2 size={16} />
          <span>{successMessage}</span>
        </div>
      )}

      {error && (
        <div className="flex items-center gap-2 rounded-lg border border-red-500/20 bg-red-500/10 p-3 text-sm text-red-400">
          <AlertCircle size={16} />
          <span>{error instanceof Error ? error.message : 'Ein Fehler ist aufgetreten'}</span>
        </div>
      )}

      <div className="flex flex-wrap gap-4 border-t border-border pt-4 text-xs text-muted">
        <span>
          <CalendarRange size={12} className="mr-1 inline" />
          Erstellt: {new Date(tournament.created_at).toLocaleString('de-DE')}
        </span>
        <span>
          Zuletzt geändert: {new Date(tournament.updated_at).toLocaleString('de-DE')}
        </span>
      </div>
    </Card>
  )
}
