import { useState } from 'react'
import type { FormEvent } from 'react'
import Card from '@/components/ui/Card'
import Button from '@/components/ui/Button'
import { useSubmitMatchResult } from '@/hooks/useTournament'
import type { BracketMatch, Team } from '@/types/tournament'
import { AlertCircle, CheckCircle, Gavel } from 'lucide-react'

interface ManualResultFormProps {
  tournamentId: number
  match: BracketMatch
  teams: Team[]
  onSuccess: () => void
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
  const [success, setSuccess] = useState(false)

  if (isTerminalMatch(match) && !allowOverride) {
    return null
  }

  const team1 = match.team1_id !== null ? teams.find(t => t.id === match.team1_id) : null
  const team2 = match.team2_id !== null ? teams.find(t => t.id === match.team2_id) : null

  const handleSubmit = async (e: FormEvent) => {
    e.preventDefault()
    if (!winnerId) return

    setIsSubmitting(true)
    setError('')
    setSuccess(false)

    try {
      await submitResultMutation.mutateAsync({
        tournamentId,
        matchId: match.id,
        data: { winner_id: winnerId },
        force: allowOverride && isTerminalMatch(match),
      })
      setSuccess(true)
      setWinnerId(null)
      onSuccess()
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
          {allowOverride && isTerminalMatch(match) ? 'Ergebnis korrigieren' : 'Manuelles Ergebnis'}
        </h4>
      </div>

      <form onSubmit={handleSubmit} className="space-y-3">
        <div className="flex flex-col gap-2">
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
        </div>

        {error && (
          <div className="flex items-center gap-2 text-red-400 text-xs bg-red-500/10 border border-red-500/20 rounded-lg p-2">
            <AlertCircle size={14} />
            <span>{error}</span>
          </div>
        )}

        {success && (
          <div className="flex items-center gap-2 text-green-400 text-xs bg-green-500/10 border border-green-500/20 rounded-lg p-2">
            <CheckCircle size={14} />
            <span>Ergebnis gespeichert</span>
          </div>
        )}

        <Button
          type="submit"
          variant="primary"
          size="sm"
          disabled={!winnerId || isSubmitting}
        >
          {isSubmitting ? 'Wird gespeichert...' : 'Ergebnis speichern'}
        </Button>
      </form>
    </Card>
  )
}
