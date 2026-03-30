import { motion } from 'framer-motion'
import { CheckCircle2, Circle, Loader2, Key } from 'lucide-react'
import type { BracketMatch as BracketMatchType, Team } from '@/types/tournament'

interface Props {
  match: BracketMatchType
  teams: Team[]
}

function getTeamName(teamId: number | null, teams: Team[], isCompleted: boolean): string {
  if (teamId === null) return isCompleted ? 'Freilos' : 'TBD'
  return teams.find(t => t.id === teamId)?.name ?? 'Unbekannt'
}

export default function BracketMatch({ match, teams }: Props) {
  const isCompleted = match.status === 'completed' || match.status === 'forfeit'
  const isLive = match.status === 'in_progress' || match.status === 'lobby_created'
  const isPending = match.status === 'pending' || match.status === 'checkin'
  const statusLabel = match.status === 'lobby_created'
    ? 'Lobby erstellt'
    : match.status === 'in_progress'
      ? 'Läuft'
      : match.status === 'checkin'
        ? 'Check-in'
        : isCompleted
          ? 'Abgeschlossen'
          : 'Ausstehend'

  const team1Name = getTeamName(match.team1_id, teams, isCompleted)
  const team2Name = getTeamName(match.team2_id, teams, isCompleted)
  const team1IsBye = match.team1_id === null
  const team2IsBye = match.team2_id === null

  const team1Won = match.winner_id !== null && match.team1_id !== null && match.winner_id === match.team1_id
  const team2Won = match.winner_id !== null && match.team2_id !== null && match.winner_id === match.team2_id
  const hasWinner = match.winner_id !== null

  return (
    <motion.div
      initial={{ opacity: 0, scale: 0.95 }}
      animate={{ opacity: 1, scale: 1 }}
      className="w-[200px] bg-card border border-border rounded-lg overflow-hidden shadow-sm flex-shrink-0"
    >
      {/* Status indicator */}
      <div className="flex items-center justify-between px-2.5 py-1 bg-card border-b border-border/50">
        {isPending && (
          <span className="flex items-center gap-1 text-[10px] text-muted">
            <Circle size={8} />
            {statusLabel}
          </span>
        )}
        {isLive && (
          <span className="flex items-center gap-1 text-[10px] text-success">
            <Loader2 size={8} className="animate-spin" />
            {statusLabel}
          </span>
        )}
        {isCompleted && (
          <span className="flex items-center gap-1 text-[10px] text-success">
            <CheckCircle2 size={8} />
            {statusLabel}
          </span>
        )}
        {match.party_code && (
          <span className="flex items-center gap-1 text-[10px] text-muted ml-auto">
            <Key size={8} />
            {match.party_code}
          </span>
        )}
      </div>

      {/* Team 1 */}
      <div
        className={`px-3 py-2 text-sm truncate ${
          team1IsBye
            ? 'text-muted italic'
            : team1Won
              ? 'font-bold text-green-400 bg-green-500/10'
              : hasWinner
                ? 'text-muted'
                : 'text-foreground'
        }`}
      >
        {team1Name}
      </div>

      {/* Divider */}
      <div className="border-t border-border/50" />

      {/* Team 2 */}
      <div
        className={`px-3 py-2 text-sm truncate ${
          team2IsBye
            ? 'text-muted italic'
            : team2Won
              ? 'font-bold text-green-400 bg-green-500/10'
              : hasWinner
                ? 'text-muted'
                : 'text-foreground'
        }`}
      >
        {team2Name}
      </div>
    </motion.div>
  )
}
