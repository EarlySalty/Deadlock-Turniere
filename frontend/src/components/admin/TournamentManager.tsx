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
  useUpdateTournament,
} from '@/hooks/useTournament'
import type { Tournament, TournamentUpdate } from '@/types/tournament'
import {
  AlertCircle,
  ArrowRight,
  CalendarRange,
  CheckCircle2,
  GitBranch,
  PencilLine,
  Shuffle,
  Trash2,
  Trophy,
  Users,
} from 'lucide-react'

interface TournamentManagerProps {
  tournament: Tournament
  teamCount: number
  playerCount: number
  matchCount: number
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

export default function TournamentManager({
  tournament,
  teamCount,
  playerCount,
  matchCount,
}: TournamentManagerProps) {
  const detailsUpdateMutation = useUpdateTournament()
  const scheduleUpdateMutation = useUpdateTournament()
  const advanceMutation = useAdvanceTournament()
  const openCheckinMutation = useOpenCheckin(tournament.id)
  const assignMutation = useAssignRandomTeams()
  const deleteMutation = useDeleteTournament()
  const generateGroupsMutation = useGenerateGroups()
  const generateBracketMutation = useGenerateBracket()

  const [form, setForm] = useState({
    name: tournament.name,
    description: tournament.description ?? '',
    team_size: tournament.team_size,
    bracket_format: tournament.bracket_format,
    registration_start: toInputDateTime(tournament.registration_start),
    registration_end: toInputDateTime(tournament.registration_end),
    group_phase_start: toInputDateTime(tournament.group_phase_start),
    bracket_start: toInputDateTime(tournament.bracket_start),
  })
  const [scheduleForm, setScheduleForm] = useState({
    registration_end: toInputDateTime(tournament.registration_end),
    group_phase_start: toInputDateTime(tournament.group_phase_start),
  })
  const [successMessage, setSuccessMessage] = useState('')
  const [scheduleSuccessMessage, setScheduleSuccessMessage] = useState('')

  const action = STATUS_ACTIONS[tournament.status]
  const isLoading =
    detailsUpdateMutation.isPending ||
    scheduleUpdateMutation.isPending ||
    advanceMutation.isPending ||
    openCheckinMutation.isPending ||
    assignMutation.isPending ||
    deleteMutation.isPending ||
    generateGroupsMutation.isPending ||
    generateBracketMutation.isPending

  const error =
    detailsUpdateMutation.error ||
    advanceMutation.error ||
    openCheckinMutation.error ||
    assignMutation.error ||
    deleteMutation.error ||
    generateGroupsMutation.error ||
    generateBracketMutation.error

  const handleChange = (
    key: keyof typeof form,
    value: string | number
  ) => {
    setForm((current) => ({ ...current, [key]: value }))
  }

  const handleSave = () => {
    const payload: TournamentUpdate = {
      name: form.name.trim(),
      description: form.description.trim() || undefined,
      team_size: form.team_size,
      bracket_format: form.bracket_format,
      registration_start: form.registration_start || undefined,
      registration_end: form.registration_end || undefined,
      group_phase_start: form.group_phase_start || undefined,
      bracket_start: form.bracket_start || undefined,
    }
    detailsUpdateMutation.mutate(
      { id: tournament.id, data: payload },
      { onSuccess: () => setSuccessMessage('Turnierdaten gespeichert.') }
    )
  }

  const handleScheduleChange = (
    key: keyof typeof scheduleForm,
    value: string,
  ) => {
    setScheduleSuccessMessage('')
    setScheduleForm((current) => ({ ...current, [key]: value }))
  }

  const handleSaveSchedule = () => {
    setScheduleSuccessMessage('')
    scheduleUpdateMutation.mutate(
      {
        id: tournament.id,
        data: {
          registration_end: scheduleForm.registration_end || undefined,
          group_phase_start: scheduleForm.group_phase_start || undefined,
        },
      },
      {
        onSuccess: () => {
          setForm((current) => ({
            ...current,
            registration_end: scheduleForm.registration_end,
            group_phase_start: scheduleForm.group_phase_start,
          }))
          setScheduleSuccessMessage('Zeitplan aktualisiert.')
        },
      },
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
    if (window.confirm(`Turnier "${tournament.name}" unwiderruflich löschen?`)) {
      deleteMutation.mutate(tournament.id)
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
        </div>

        <div className="grid gap-4 sm:grid-cols-2">
          <div>
            <label htmlFor="admin-team-size" className="mb-1.5 block text-sm font-medium text-foreground">
              Teamgröße
            </label>
            <select
              id="admin-team-size"
              value={form.team_size}
              onChange={(event) => handleChange('team_size', Number(event.target.value))}
              className="w-full rounded-lg border border-border bg-background px-3 py-2 text-foreground focus:outline-none focus:ring-2 focus:ring-primary/50"
            >
              {[2, 3, 4, 5, 6].map((size) => (
                <option key={size} value={size}>
                  {size} Spieler
                </option>
              ))}
            </select>
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
              Anmeldung Ende
            </label>
            <DateTimeInput
              id="admin-reg-end"
              value={form.registration_end}
              onChange={(event) => handleChange('registration_end', event.target.value)}
              className="w-full rounded-lg border border-border bg-background px-3 py-2 text-foreground focus:outline-none focus:ring-2 focus:ring-primary/50"
            />
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

      {['registration', 'checkin', 'group_phase'].includes(tournament.status) && (
        <div className="rounded-xl border border-primary/20 bg-primary/5 p-5 space-y-4">
          <div>
            <h3 className="text-base font-semibold text-foreground">Zeitplan anpassen</h3>
            <p className="mt-1 text-sm text-muted">
              Anmeldung verlängern oder den Start der Gruppenphase direkt verschieben.
            </p>
          </div>

          <div className="grid gap-4 md:grid-cols-2">
            <div>
              <label htmlFor="quick-registration-end" className="mb-1.5 block text-sm font-medium text-foreground">
                Anmeldung Ende
              </label>
              <DateTimeInput
                id="quick-registration-end"
                value={scheduleForm.registration_end}
                onChange={(event) => handleScheduleChange('registration_end', event.target.value)}
                className="w-full rounded-lg border border-border bg-background px-3 py-2 text-foreground focus:outline-none focus:ring-2 focus:ring-primary/50"
              />
            </div>

            <div>
              <label htmlFor="quick-group-start" className="mb-1.5 block text-sm font-medium text-foreground">
                Gruppenphase Start
              </label>
              <DateTimeInput
                id="quick-group-start"
                value={scheduleForm.group_phase_start}
                onChange={(event) => handleScheduleChange('group_phase_start', event.target.value)}
                className="w-full rounded-lg border border-border bg-background px-3 py-2 text-foreground focus:outline-none focus:ring-2 focus:ring-primary/50"
              />
            </div>
          </div>

          <div className="flex flex-wrap items-center gap-3">
            <Button
              variant="primary"
              size="sm"
              disabled={isLoading}
              onClick={handleSaveSchedule}
            >
              <CalendarRange size={14} />
              {scheduleUpdateMutation.isPending ? 'Speichert...' : 'Zeitplan speichern'}
            </Button>

            {scheduleSuccessMessage && (
              <div className="flex items-center gap-2 rounded-lg border border-green-500/20 bg-green-500/10 px-3 py-2 text-sm text-green-400">
                <CheckCircle2 size={16} />
                <span>{scheduleSuccessMessage}</span>
              </div>
            )}

            {scheduleUpdateMutation.error && (
              <div className="flex items-center gap-2 rounded-lg border border-red-500/20 bg-red-500/10 px-3 py-2 text-sm text-red-400">
                <AlertCircle size={16} />
                <span>
                  {scheduleUpdateMutation.error instanceof Error
                    ? scheduleUpdateMutation.error.message
                    : 'Zeitplan konnte nicht aktualisiert werden'}
                </span>
              </div>
            )}
          </div>
        </div>
      )}

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

        {action && tournament.status !== 'archived' && (
          <Button variant="primary" size="sm" disabled={isLoading} onClick={handleAdvance}>
            <ArrowRight size={14} />
            {advanceMutation.isPending ? 'Verarbeitet...' : action.label}
          </Button>
        )}

        {['draft', 'completed', 'archived'].includes(tournament.status) && (
          <Button variant="danger" size="sm" disabled={isLoading} onClick={handleDelete}>
            <Trash2 size={14} />
            {deleteMutation.isPending ? 'Löscht...' : 'Turnier löschen'}
          </Button>
        )}
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
