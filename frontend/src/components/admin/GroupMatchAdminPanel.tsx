import { useMemo, useState } from 'react'
import Card from '@/components/ui/Card'
import Button from '@/components/ui/Button'
import { useSubmitMatchResult } from '@/hooks/useTournament'
import type { Group, Team } from '@/types/tournament'
import { AlertCircle, CheckCircle2, Gavel, Swords } from 'lucide-react'

interface GroupMatchAdminPanelProps {
  tournamentId: number
  groups: Group[]
  teams: Team[]
  onRefresh?: () => void
}

type MatchMessage = {
  kind: 'success' | 'error'
  text: string
}

function getTeamName(teamId: number, teams: Team[], group: Group): string {
  const groupedTeam = group.teams.find((team) => team.team_id === teamId)
  if (groupedTeam) return groupedTeam.team_name
  return teams.find((team) => team.id === teamId)?.name ?? `Team #${teamId}`
}

function getStatusLabel(status: string): string {
  switch (status) {
    case 'completed':
      return 'Abgeschlossen'
    case 'forfeit':
      return 'Forfeit'
    case 'cancelled':
      return 'Abgesagt'
    default:
      return 'Ausstehend'
  }
}

export default function GroupMatchAdminPanel({
  tournamentId,
  groups,
  teams,
  onRefresh,
}: GroupMatchAdminPanelProps) {
  const submitResultMutation = useSubmitMatchResult()
  const [selectedWinners, setSelectedWinners] = useState<Record<number, number>>({})
  const [submittingMatchId, setSubmittingMatchId] = useState<number | null>(null)
  const [messages, setMessages] = useState<Record<number, MatchMessage>>({})

  const groupsWithMatches = useMemo(
    () => groups.filter((group) => group.matches.length > 0),
    [groups],
  )

  if (groupsWithMatches.length === 0) {
    return (
      <Card className="p-5">
        <div className="flex items-center gap-2 text-muted">
          <Swords size={18} />
          <span>Keine Gruppenspiele vorhanden.</span>
        </div>
      </Card>
    )
  }

  const setMessage = (matchId: number, message: MatchMessage) => {
    setMessages((current) => ({ ...current, [matchId]: message }))
  }

  const clearMessage = (matchId: number) => {
    setMessages((current) => {
      const next = { ...current }
      delete next[matchId]
      return next
    })
  }

  const handleSubmit = async (matchId: number) => {
    const winnerId = selectedWinners[matchId]
    if (!winnerId || submittingMatchId !== null) return

    setSubmittingMatchId(matchId)
    clearMessage(matchId)

    try {
      await submitResultMutation.mutateAsync({
        tournamentId,
        matchId,
        data: { winner_id: winnerId },
      })
      setMessage(matchId, { kind: 'success', text: 'Ergebnis gespeichert.' })
      setSelectedWinners((current) => {
        const next = { ...current }
        delete next[matchId]
        return next
      })
      onRefresh?.()
    } catch (error) {
      setMessage(matchId, {
        kind: 'error',
        text: error instanceof Error ? error.message : 'Ergebnis konnte nicht gespeichert werden',
      })
    } finally {
      setSubmittingMatchId(null)
    }
  }

  return (
    <div className="grid gap-4">
      {groupsWithMatches.map((group) => (
        <Card key={group.id} className="p-5 space-y-4">
          <div className="flex items-center justify-between gap-3">
            <div>
              <h3 className="text-lg font-semibold text-foreground">{group.name}</h3>
              <p className="mt-1 text-sm text-muted">
                Ergebnisse der Gruppenphase eintragen oder prüfen.
              </p>
            </div>
            <span className="text-xs text-muted">
              {group.matches.filter((match) => match.status === 'completed').length}/{group.matches.length} gespielt
            </span>
          </div>

          <div className="space-y-3">
            {group.matches.map((match) => {
              const team1Name = getTeamName(match.team1_id, teams, group)
              const team2Name = getTeamName(match.team2_id, teams, group)
              const winnerName = match.winner_id
                ? getTeamName(match.winner_id, teams, group)
                : null
              const message = messages[match.id]
              const isTerminal = ['completed', 'forfeit', 'cancelled'].includes(match.status)
              const isSubmitting = submittingMatchId === match.id

              return (
                <div key={match.id} className="rounded-xl border border-border bg-background/60 p-4">
                  <div className="flex flex-col gap-2 md:flex-row md:items-center md:justify-between">
                    <div>
                      <div className="flex items-center gap-2 text-sm text-muted">
                        <Gavel size={14} className="text-primary" />
                        <span>Match #{match.id}</span>
                      </div>
                      <h4 className="mt-1 text-base font-semibold text-foreground">
                        {team1Name} vs {team2Name}
                      </h4>
                    </div>
                    <div className="text-sm text-muted">
                      Status: <span className="text-foreground">{getStatusLabel(match.status)}</span>
                    </div>
                  </div>

                  {winnerName && (
                    <p className="mt-3 text-sm text-success">
                      Gewinner: <span className="font-semibold">{winnerName}</span>
                    </p>
                  )}

                  {!isTerminal && (
                    <div className="mt-4 space-y-3">
                      <div className="space-y-2">
                        <label className="flex items-center gap-2 text-sm text-foreground">
                          <input
                            type="radio"
                            name={`group-winner-${match.id}`}
                            checked={selectedWinners[match.id] === match.team1_id}
                            onChange={() =>
                              setSelectedWinners((current) => ({ ...current, [match.id]: match.team1_id }))
                            }
                            className="accent-primary"
                          />
                          <span>{team1Name} gewonnen</span>
                        </label>
                        <label className="flex items-center gap-2 text-sm text-foreground">
                          <input
                            type="radio"
                            name={`group-winner-${match.id}`}
                            checked={selectedWinners[match.id] === match.team2_id}
                            onChange={() =>
                              setSelectedWinners((current) => ({ ...current, [match.id]: match.team2_id }))
                            }
                            className="accent-primary"
                          />
                          <span>{team2Name} gewonnen</span>
                        </label>
                      </div>

                      <Button
                        variant="primary"
                        size="sm"
                        disabled={!selectedWinners[match.id] || submittingMatchId !== null}
                        onClick={() => void handleSubmit(match.id)}
                      >
                        {isSubmitting ? 'Speichert...' : 'Ergebnis speichern'}
                      </Button>
                    </div>
                  )}

                  {message && (
                    <div
                      className={`mt-3 flex items-center gap-2 rounded-lg p-3 text-sm ${
                        message.kind === 'error'
                          ? 'border border-red-500/20 bg-red-500/10 text-red-400'
                          : 'border border-green-500/20 bg-green-500/10 text-green-400'
                      }`}
                    >
                      {message.kind === 'error' ? <AlertCircle size={16} /> : <CheckCircle2 size={16} />}
                      <span>{message.text}</span>
                    </div>
                  )}
                </div>
              )
            })}
          </div>
        </Card>
      ))}
    </div>
  )
}
