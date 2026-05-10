import { motion } from 'framer-motion'
import { CheckCircle2, Circle, Loader2, Key } from 'lucide-react'
import TeamRosterTooltip from '@/components/ui/TeamRosterTooltip'
import type { BracketMatch as BracketMatchType, TeamPublic } from '@/types/tournament'

interface Props {
  match: BracketMatchType
  teams: TeamPublic[]
  placeholder1?: string | null
  placeholder2?: string | null
}

function findTeam(teamId: number | null, teams: TeamPublic[]): TeamPublic | null {
  if (teamId === null) return null
  return teams.find((team) => team.id === teamId) ?? null
}

function teamLabel(
  team: TeamPublic | null,
  teamId: number | null,
  isCompleted: boolean,
  placeholder?: string | null,
): string {
  if (teamId === null) {
    if (isCompleted) return 'Freilos'
    return placeholder ?? 'TBD'
  }
  return team?.name ?? 'Unbekannt'
}

export default function BracketMatch({ match, teams, placeholder1, placeholder2 }: Props) {
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

  const team1 = findTeam(match.team1_id, teams)
  const team2 = findTeam(match.team2_id, teams)
  const team1Name = teamLabel(team1, match.team1_id, isCompleted, placeholder1)
  const team2Name = teamLabel(team2, match.team2_id, isCompleted, placeholder2)
  const team1IsBye = match.team1_id === null
  const team2IsBye = match.team2_id === null
  const team1IsPlaceholder = match.team1_id === null && !isCompleted && !!placeholder1
  const team2IsPlaceholder = match.team2_id === null && !isCompleted && !!placeholder2

  const team1Won = match.winner_id !== null && match.team1_id !== null && match.winner_id === match.team1_id
  const team2Won = match.winner_id !== null && match.team2_id !== null && match.winner_id === match.team2_id
  const hasWinner = match.winner_id !== null

  const team1Class = team1IsPlaceholder
    ? 'text-muted/70 italic text-xs'
    : team1IsBye
      ? 'text-muted italic'
      : team1Won
        ? 'font-bold text-green-400 bg-green-500/10'
        : hasWinner
          ? 'text-muted'
          : 'text-foreground'

  const team2Class = team2IsPlaceholder
    ? 'text-muted/70 italic text-xs'
    : team2IsBye
      ? 'text-muted italic'
      : team2Won
        ? 'font-bold text-green-400 bg-green-500/10'
        : hasWinner
          ? 'text-muted'
          : 'text-foreground'

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
      <TeamRosterTooltip
        teamName={team1Name}
        members={team1?.members ?? []}
        align="left"
        disabled={team1IsBye || team1IsPlaceholder}
        block
      >
        <div className={`px-3 py-2 text-sm truncate ${team1Class}`}>
          {team1Name}
        </div>
      </TeamRosterTooltip>

      {/* Divider */}
      <div className="border-t border-border/50" />

      {/* Team 2 */}
      <TeamRosterTooltip
        teamName={team2Name}
        members={team2?.members ?? []}
        align="left"
        disabled={team2IsBye || team2IsPlaceholder}
        block
      >
        <div className={`px-3 py-2 text-sm truncate ${team2Class}`}>
          {team2Name}
        </div>
      </TeamRosterTooltip>
    </motion.div>
  )
}
