import Card from '@/components/ui/Card'
import Badge from '@/components/ui/Badge'
import Button from '@/components/ui/Button'
import {
  useAdvanceTournament,
  useAssignRandomTeams,
  useDeleteTournament,
} from '@/hooks/useTournament'
import type { Tournament } from '@/types/tournament'
import { Settings, Users, Shuffle, ArrowRight, Trash2, AlertCircle } from 'lucide-react'

interface TournamentManagerProps {
  tournament: Tournament
  teamCount: number
  playerCount: number
  matchCount: number
}

const STATUS_ACTIONS: Record<string, { label: string; confirmMsg: string }> = {
  draft: { label: 'Anmeldung starten', confirmMsg: 'Turnier-Anmeldung wirklich starten?' },
  registration: { label: 'Zur Gruppenphase', confirmMsg: 'Anmeldung beenden und Gruppenphase starten?' },
  group_phase: { label: 'Zum Bracket', confirmMsg: 'Gruppenphase beenden und Bracket starten?' },
  bracket: { label: 'Turnier abschliessen', confirmMsg: 'Turnier wirklich abschliessen?' },
  completed: { label: 'Archivieren', confirmMsg: 'Turnier ins Archiv verschieben?' },
}

export default function TournamentManager({
  tournament,
  teamCount,
  playerCount,
  matchCount,
}: TournamentManagerProps) {
  const advanceMutation = useAdvanceTournament()
  const assignMutation = useAssignRandomTeams()
  const deleteMutation = useDeleteTournament()

  const action = STATUS_ACTIONS[tournament.status]
  const isLoading = advanceMutation.isPending || assignMutation.isPending || deleteMutation.isPending
  const error = advanceMutation.error || assignMutation.error || deleteMutation.error

  const handleAdvance = () => {
    if (!action) return
    if (window.confirm(action.confirmMsg)) {
      advanceMutation.mutate(tournament.id)
    }
  }

  const handleAssignRandom = () => {
    if (window.confirm('Solo-Spieler zufällig auf Teams verteilen?')) {
      assignMutation.mutate(tournament.id)
    }
  }

  const handleDelete = () => {
    if (window.confirm(`Turnier "${tournament.name}" unwiderruflich löschen?`)) {
      deleteMutation.mutate(tournament.id)
    }
  }

  return (
    <Card className="p-6">
      <div className="flex items-center justify-between mb-6">
        <div className="flex items-center gap-3">
          <Settings size={20} className="text-primary" />
          <h2 className="text-lg font-semibold text-foreground">Aktives Turnier verwalten</h2>
        </div>
      </div>

      {/* Turnier-Info */}
      <div className="flex items-center gap-3 mb-5">
        <h3 className="text-xl font-bold text-foreground">{tournament.name}</h3>
        <Badge status={tournament.status} />
      </div>

      {tournament.description && (
        <p className="text-muted text-sm mb-5">{tournament.description}</p>
      )}

      {/* Stats */}
      <div className="grid grid-cols-2 sm:grid-cols-4 gap-3 mb-6">
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

      {/* Action Buttons */}
      <div className="flex flex-wrap gap-2">
        {tournament.status === 'registration' && (
          <Button
            variant="secondary"
            size="sm"
            onClick={handleAssignRandom}
            disabled={isLoading}
          >
            <Shuffle size={14} />
            {assignMutation.isPending ? 'Wird zugewiesen...' : 'Solo-Spieler zuweisen'}
          </Button>
        )}

        {action && tournament.status !== 'archived' && (
          <Button
            variant="primary"
            size="sm"
            onClick={handleAdvance}
            disabled={isLoading}
          >
            <ArrowRight size={14} />
            {advanceMutation.isPending ? 'Wird verarbeitet...' : action.label}
          </Button>
        )}

        {tournament.status === 'draft' && (
          <Button
            variant="danger"
            size="sm"
            onClick={handleDelete}
            disabled={isLoading}
          >
            <Trash2 size={14} />
            {deleteMutation.isPending ? 'Wird gelöscht...' : 'Löschen'}
          </Button>
        )}
      </div>

      {/* Assign-Success */}
      {assignMutation.isSuccess && assignMutation.data && (
        <div className="mt-4 text-green-400 text-sm bg-green-500/10 border border-green-500/20 rounded-lg p-3">
          {assignMutation.data.teams_created} Teams wurden erstellt.
        </div>
      )}

      {/* Fehler */}
      {error && (
        <div className="mt-4 flex items-center gap-2 text-red-400 text-sm bg-red-500/10 border border-red-500/20 rounded-lg p-3">
          <AlertCircle size={16} />
          <span>{error instanceof Error ? error.message : 'Ein Fehler ist aufgetreten'}</span>
        </div>
      )}

      {/* Meta-Info */}
      <div className="mt-6 pt-4 border-t border-border text-xs text-muted flex flex-wrap gap-4">
        <span>
          <Users size={12} className="inline mr-1" />
          Format: {tournament.bracket_format === 'single_elimination' ? 'Single Elimination' : 'Double Elimination'}
        </span>
        <span>
          Erstellt: {new Date(tournament.created_at).toLocaleDateString('de-DE')}
        </span>
        {tournament.registration_start && (
          <span>
            Anmeldung ab: {new Date(tournament.registration_start).toLocaleString('de-DE')}
          </span>
        )}
        {tournament.registration_end && (
          <span>
            Anmeldung bis: {new Date(tournament.registration_end).toLocaleString('de-DE')}
          </span>
        )}
      </div>
    </Card>
  )
}
