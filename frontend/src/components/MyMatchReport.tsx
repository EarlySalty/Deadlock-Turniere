import { useState } from 'react'
import { CheckCircle2, Send, UserX } from 'lucide-react'
import Card from '@/components/ui/Card'
import Button from '@/components/ui/Button'
import { useReportMatchResult } from '@/hooks/useTournament'
import type { BracketMatch, TeamPublic } from '@/types/tournament'

interface MyMatchReportProps {
  tournamentId: number
  matches: BracketMatch[]
  teams: TeamPublic[]
  myTeamId: number
}

const TERMINAL = ['completed', 'forfeit', 'cancelled']

function teamName(teamId: number | null, teams: TeamPublic[]): string {
  if (teamId === null) return 'TBD'
  return teams.find((t) => t.id === teamId)?.name ?? `Team #${teamId}`
}

function MatchReportRow({
  tournamentId,
  match,
  teams,
  myTeamId,
}: {
  tournamentId: number
  match: BracketMatch
  teams: TeamPublic[]
  myTeamId: number
}) {
  const opponentId = match.team1_id === myTeamId ? match.team2_id : match.team1_id
  const [winnerId, setWinnerId] = useState<number | null>(null)
  const [deadlockMatchId, setDeadlockMatchId] = useState('')
  const report = useReportMatchResult(tournamentId)

  const submitResult = () => {
    if (winnerId === null) return
    report.mutate({
      matchId: match.id,
      data: {
        winner_team_id: winnerId,
        deadlock_match_id: deadlockMatchId.trim() || null,
      },
    })
  }

  const submitNoShow = () => {
    if (opponentId === null) return
    report.mutate({
      matchId: match.id,
      data: { is_no_show: true, no_show_team_id: opponentId },
    })
  }

  if (report.isSuccess) {
    return (
      <div className="rounded-lg border border-green-500/20 bg-green-500/10 p-3 text-sm text-green-400">
        <div className="flex items-center gap-2">
          <CheckCircle2 size={15} />
          <span>
            Match {match.id} gemeldet — ein Admin bestätigt das Ergebnis gleich.
          </span>
        </div>
      </div>
    )
  }

  return (
    <div className="rounded-lg border border-border bg-white/[0.02] p-3">
      <div className="text-sm font-semibold text-foreground">
        Match {match.id} — Runde {match.round}
      </div>
      <div className="mt-0.5 text-xs text-muted">
        {teamName(match.team1_id, teams)} vs {teamName(match.team2_id, teams)}
      </div>

      <div className="mt-2.5 flex flex-wrap gap-2">
        <button
          type="button"
          onClick={() => setWinnerId(myTeamId)}
          className={`rounded-lg border px-3 py-1.5 text-xs font-medium transition-colors ${
            winnerId === myTeamId
              ? 'border-primary bg-primary/15 text-primary'
              : 'border-border text-foreground/80 hover:bg-white/5'
          }`}
        >
          Wir gewinnen
        </button>
        <button
          type="button"
          onClick={() => opponentId !== null && setWinnerId(opponentId)}
          className={`rounded-lg border px-3 py-1.5 text-xs font-medium transition-colors ${
            winnerId === opponentId
              ? 'border-primary bg-primary/15 text-primary'
              : 'border-border text-foreground/80 hover:bg-white/5'
          }`}
        >
          {teamName(opponentId, teams)} gewinnt
        </button>
      </div>

      <input
        type="text"
        value={deadlockMatchId}
        onChange={(e) => setDeadlockMatchId(e.target.value)}
        placeholder="Deadlock Match-ID (zur Nachprüfung)"
        className="mt-2 w-full rounded-lg border border-border bg-background px-3 py-2 text-sm text-foreground focus:outline-none focus:ring-2 focus:ring-primary/40"
      />

      <div className="mt-2.5 flex flex-wrap gap-2">
        <Button
          variant="primary"
          size="sm"
          disabled={winnerId === null || report.isPending}
          onClick={submitResult}
        >
          <Send size={13} />
          Ergebnis melden
        </Button>
        <Button
          variant="ghost"
          size="sm"
          disabled={opponentId === null || report.isPending}
          onClick={submitNoShow}
        >
          <UserX size={13} />
          Gegner nicht erschienen
        </Button>
      </div>

      {report.isError && (
        <p className="mt-2 text-xs text-red-400">
          {(report.error as Error)?.message || 'Meldung fehlgeschlagen'}
        </p>
      )}
    </div>
  )
}

export default function MyMatchReport({
  tournamentId,
  matches,
  teams,
  myTeamId,
}: MyMatchReportProps) {
  const myMatches = matches.filter(
    (m) =>
      (m.team1_id === myTeamId || m.team2_id === myTeamId) &&
      m.team1_id !== null &&
      m.team2_id !== null &&
      !TERMINAL.includes(m.status),
  )

  if (myMatches.length === 0) return null

  return (
    <Card className="space-y-3 p-5">
      <div>
        <h3 className="text-sm font-bold uppercase tracking-widest text-foreground">
          Deine Matches melden
        </h3>
        <p className="mt-1 text-xs text-muted">
          Tragt nach dem Spiel selbst ein, wer gewonnen hat — am besten mit der
          Deadlock-Match-ID. Ein Admin bestätigt das Ergebnis anschließend.
        </p>
      </div>
      <div className="space-y-2.5">
        {myMatches.map((match) => (
          <MatchReportRow
            key={match.id}
            tournamentId={tournamentId}
            match={match}
            teams={teams}
            myTeamId={myTeamId}
          />
        ))}
      </div>
    </Card>
  )
}
