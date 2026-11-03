import { useState } from 'react'
import Card from '@/components/ui/Card'
import Badge from '@/components/ui/Badge'
import Button from '@/components/ui/Button'
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
import { LEAGUE_LABEL, isLeague, leagueFinishConfirm } from '@/lib/league'
import type { Tournament } from '@/types/tournament'
import ChoiceCards from './settings/ChoiceCards'
import GameRulesFields from './settings/GameRulesFields'
import SpecialSettingsFields from './settings/SpecialSettingsFields'
import { SettingsSection, SubHeading } from './settings/SettingsSection'
import { BasicsFields, DateFields, MatchFormatFields, ReminderFields } from './settings/StandardFields'
import { buildUpdatePayload, editDateEntries, initialEditState, type EditForm } from './settings/editForm'
import { MODE_OPTIONS } from './settings/options'
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
  canChangeTournamentMode: boolean
}

const STATUS_ACTIONS: Record<string, { label: string; confirmMsg: string }> = {
  draft: { label: 'Anmeldung starten', confirmMsg: 'Turnier-Anmeldung wirklich starten?' },
  group_phase: { label: 'Zum Bracket', confirmMsg: 'Gruppenphase beenden und Bracket starten?' },
  bracket: { label: 'Turnier abschliessen', confirmMsg: 'Turnier wirklich abschliessen?' },
  completed: { label: 'Archivieren', confirmMsg: 'Turnier ins Archiv verschieben?' },
}

function leagueFinishAction(countsForLeaderboard: boolean) {
  return {
    label: 'Turnier abschließen',
    confirmMsg: `${leagueFinishConfirm(countsForLeaderboard)} Das geht erst, wenn alle Spiele gewertet sind.`,
  }
}

const LEAGUE_STEPS = [
  'Anmeldung und Check-in laufen wie gewohnt.',
  'Beim Abschluss des Check-ins entsteht der Spielplan: jedes Team gegen jedes andere, in festen Runden ab dem eingestellten Start.',
  'Unter „Spielplan“ steuerst du Zeiten und Lobbys und trägst Sieger ein oder korrigierst sie. Sieg +2, Niederlage -1.',
  'Sind alle Spiele gewertet, schließt du das Turnier ab. Die Tabelle ist die Endplatzierung.',
]

type SetupTab = 'settings' | 'rules'

const SETUP_TABS: { key: SetupTab; label: string }[] = [
  { key: 'settings', label: 'Einstellungen' },
  { key: 'rules', label: 'Spielregeln' },
]

const lastSetupTab = new Map<number, SetupTab>()

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

  const storedLeague = isLeague(tournament.tournament_mode)
  const [initial] = useState(() => initialEditState(tournament))
  const [form, setForm] = useState<EditForm>(initial.form)
  const [special, setSpecial] = useState(initial.special)
  const [gameRules, setGameRules] = useState(initial.gameRules)
  const [customJsonError, setCustomJsonError] = useState('')
  const [setupTab, setSetupTab] = useState<SetupTab>(() => lastSetupTab.get(tournament.id) ?? 'settings')
  const [successMessage, setSuccessMessage] = useState('')
  const [deleteConfirmationId, setDeleteConfirmationId] = useState<number | null>(null)
  const deleteConfirmed = deleteConfirmationId === tournament.id

  const league = isLeague(form.tournamentMode)
  const leagueRunning = tournament.status === 'group_phase'
  const modeLocked = !canChangeTournamentMode || (leagueRunning && storedLeague)
  const action =
    storedLeague && leagueRunning
      ? leagueFinishAction(!tournament.exclude_from_leaderboard)
      : STATUS_ACTIONS[tournament.status]
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

  const update = (patch: Partial<EditForm>) => setForm((current) => ({ ...current, ...patch }))

  const selectTab = (tab: SetupTab) => {
    lastSetupTab.set(tournament.id, tab)
    setSetupTab(tab)
  }

  const handleSave = () => {
    const result = buildUpdatePayload({ form, special, gameRules }, initial, tournament, canChangeTournamentMode)
    if (!result.ok) {
      setCustomJsonError(result.error)
      selectTab('rules')
      return
    }
    detailsUpdateMutation.mutate(
      { id: tournament.id, data: result.payload },
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
        setDeleteConfirmationId(tournament.id)
      }
    } else {
      deleteMutation.mutate(tournament.id, {
        onSuccess: () => {
          setDeleteConfirmationId(null)
          setSuccessMessage('Turnier wurde gelöscht.')
        },
        onError: () => {
          setDeleteConfirmationId(null)
        }
      })
    }
  }

  return (
    <Card className="space-y-6 sm:p-2">
      <div className="flex flex-col gap-4 lg:flex-row lg:items-start lg:justify-between">
        <div className="space-y-2">
          <div className="flex items-center gap-3">
            <Trophy size={20} className="text-primary" />
            <h2 className="text-lg font-semibold text-foreground">Turnier-Verwaltung</h2>
            <Badge status={tournament.status} mode={tournament.tournament_mode} />
          </div>
          <p className="text-sm text-muted">
            Einstellungen und Spielregeln pflegen, Phasen steuern und Generierung sicher auslösen.
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

      {league && (
        <div className="rounded-lg border border-primary/20 bg-primary/5 p-4">
          <h3 className="text-sm font-semibold text-foreground">So läuft „{LEAGUE_LABEL}“</h3>
          <ol className="mt-2 list-decimal space-y-1 pl-5 text-sm text-muted">
            {LEAGUE_STEPS.map((step) => (
              <li key={step}>{step}</li>
            ))}
          </ol>
        </div>
      )}

      <div role="tablist" aria-label="Turnier bearbeiten" className="flex flex-wrap gap-1 border-b border-border">
        {SETUP_TABS.map((tab) => (
          <button
            key={tab.key}
            type="button"
            role="tab"
            aria-selected={setupTab === tab.key}
            onClick={() => selectTab(tab.key)}
            className={`border-b-2 px-4 py-2.5 text-sm font-medium transition-colors ${
              setupTab === tab.key
                ? 'border-primary text-primary'
                : 'border-transparent text-muted hover:text-foreground'
            }`}
          >
            {tab.label}
          </button>
        ))}
      </div>

      {setupTab === 'settings' ? (
        <div className="space-y-5">
          <SettingsSection
            title="Standard-Einstellungen"
            description="Das Wichtigste für jedes Turnier: Name, Ablauf, Termine und Erinnerungen."
          >
            <BasicsFields idPrefix="admin" value={form} onChange={update} />
            <div className="space-y-4">
              <SubHeading>Ablauf</SubHeading>
              <ChoiceCards
                name="admin-tournament-mode"
                legend="Turniermodus"
                options={MODE_OPTIONS}
                value={form.tournamentMode}
                onChange={(tournamentMode) => update({ tournamentMode })}
                disabled={modeLocked}
                isOptionDisabled={(mode) => mode === 'league' && leagueRunning && !storedLeague}
                hint={
                  league
                    ? 'Festlegen geht bis zum Abschluss des Check-ins.'
                    : 'Änderbar im Check-in oder in der Gruppenphase, solange noch kein Gruppenspiel gespielt wurde.'
                }
              />
              <MatchFormatFields idPrefix="admin" value={form} onChange={update} league={league} />
            </div>
            <DateFields idPrefix="admin" entries={editDateEntries(form, update, league)} />
            <ReminderFields idPrefix="admin" value={form} onChange={update} />
          </SettingsSection>

          <SettingsSection
            title="Spezielle Einstellungen"
            description="Nur ändern, wenn das Turnier von den üblichen Regeln abweicht."
          >
            <SpecialSettingsFields idPrefix="admin" value={special} onChange={setSpecial} />
          </SettingsSection>
        </div>
      ) : (
        <SettingsSection
          title="Spielregeln"
          description="Was in jeder Lobby gilt. Änderungen wirken auf alle Lobbys, die ab jetzt erstellt werden."
        >
          <GameRulesFields
            idPrefix="admin"
            value={gameRules}
            onChange={(next) => {
              setGameRules(next)
              setCustomJsonError('')
            }}
            league={league}
            customJsonError={customJsonError}
          />
        </SettingsSection>
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

        {tournament.status === 'group_phase' && !storedLeague && (
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
