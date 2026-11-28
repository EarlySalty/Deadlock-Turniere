import { useState } from 'react'
import type { FormEvent } from 'react'
import Card from '@/components/ui/Card'
import Button from '@/components/ui/Button'
import { useSubmitMatchResult } from '@/hooks/useTournament'
import type { BracketMatch, Team } from '@/types/tournament'
import { AlertCircle, Gavel } from 'lucide-react'

interface ManualResultFormProps {
  tournamentId: number
  match: BracketMatch
  teams: Team[]
  onSuccess: (message: string) => void
  allowOverride?: boolean
}

function isTerminalMatch(match: BracketMatch): boolean {
  return ['completed', 'forfeit', 'cancelled'].includes(match.status)
}

export default function ManualResultForm({
  tournamentId,
  match,
  teams,
  onSuccess,
  allowOverride = false,
}: ManualResultFormProps) {
  const submitResultMutation = useSubmitMatchResult()
  const [winnerId, setWinnerId] = useState<number | null>(null)
  const [isSubmitting, setIsSubmitting] = useState(false)
  const [error, setError] = useState('')

  if (isTerminalMatch(match) && !allowOverride) {
    return null
  }

  const team1 = match.team1_id !== null ? teams.find(t => t.id === match.team1_id) : null
  const team2 = match.team2_id !== null ? teams.find(t => t.id === match.team2_id) : null
  const isOverride = allowOverride && isTerminalMatch(match)
  const currentWinner = teams.find((team) => team.id === match.winner_id) ?? null
  const unchangedWinner = isOverride && winnerId !== null && winnerId === match.winner_id

  const confirmOverride = (nextWinnerId: number): boolean => {
    if (!isOverride || !currentWinner) return true
    const nextWinner = teams.find((team) => team.id === nextWinnerId)
    return window.confirm(
      `Ergebnis überschreiben? Bisher gewinnt ${currentWinner.name}, neu gewinnt ${nextWinner?.name ?? 'das andere Team'}. Spätere Matches, in die dieses Ergebnis schon eingegangen ist, werden zurückgesetzt und müssen neu gespielt oder eingetragen werden.`,
    )
  }

  const handleSubmit = async (e: FormEvent) => {
    e.preventDefault()
    if (!winnerId || unchangedWinner || !confirmOverride(winnerId)) return

    setIsSubmitting(true)
    setError('')

    try {
      await submitResultMutation.mutateAsync({
        tournamentId,
        matchId: match.id,
        data: { winner_id: winnerId },
        force: isOverride,
      })
      setWinnerId(null)
      onSuccess(isOverride ? 'Ergebnis korrigiert.' : 'Ergebnis gespeichert.')
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Fehler beim Speichern')
    } finally {
      setIsSubmitting(false)
    }
  }

  return (
    <Card className="p-4">
      <div className="flex items-center gap-2 mb-3">
        <Gavel size={16} className="text-primary" />
        <h4 className="text-sm font-semibold text-foreground">
          {isOverride ? 'Ergebnis korrigieren' : 'Manuelles Ergebnis'}
        </h4>
      </div>
      <p className="mb-3 text-xs text-muted">
        {isOverride
          ? `Aktuell eingetragen: ${currentWinner ? `${currentWinner.name} gewinnt` : 'kein Sieger'}.`
          : 'Dieses Ergebnis kann auch ohne Bot-Lobby oder automatischen Match-Start direkt gesetzt werden.'}
      </p>

      <form onSubmit={handleSubmit} className="space-y-3">
        <fieldset className="flex flex-col gap-2">
          <legend className="mb-1 text-xs font-medium uppercase tracking-wider text-muted">Sieger</legend>
          {team1 && (
            <label className="flex items-center gap-2 cursor-pointer">
              <input
                type="radio"
                name={`winner-${match.id}`}
                value={team1.id}
                checked={winnerId === team1.id}
                onChange={() => setWinnerId(team1.id)}
                className="accent-primary"
              />
              <span className="text-foreground text-sm">{team1.name}</span>
            </label>
          )}
          {team2 && (
            <label className="flex items-center gap-2 cursor-pointer">
              <input
                type="radio"
                name={`winner-${match.id}`}
                value={team2.id}
                checked={winnerId === team2.id}
                onChange={() => setWinnerId(team2.id)}
                className="accent-primary"
              />
              <span className="text-foreground text-sm">{team2.name}</span>
            </label>
          )}
        </fieldset>

        {unchangedWinner && (
          <p className="text-xs text-muted">Dieses Team ist bereits als Sieger eingetragen.</p>
        )}

        {error && (
          <div role="alert" className="flex items-center gap-2 text-red-400 text-xs bg-red-500/10 border border-red-500/20 rounded-lg p-2">
            <AlertCircle size={14} />
            <span>{error}</span>
          </div>
        )}

        <Button
          type="submit"
          variant="primary"
          size="sm"
          disabled={!winnerId || unchangedWinner || isSubmitting}
        >
          {isSubmitting ? 'Wird gespeichert...' : 'Ergebnis speichern'}
        </Button>
      </form>
    </Card>
  )
}
