import { motion } from 'framer-motion'
import { Users2, ArrowRight } from 'lucide-react'
import Card from '@/components/ui/Card'
import TeamRosterTooltip from '@/components/ui/TeamRosterTooltip'
import type { BracketMatch, BracketMiniGroup, TeamPublic } from '@/types/tournament'

interface MiniGroupPanelProps {
  miniGroups: BracketMiniGroup[]
  matches: BracketMatch[]
  teams: TeamPublic[]
}

interface MiniGroupStanding {
  teamId: number
  team: TeamPublic | null
  wins: number
  losses: number
  played: number
}

function calcStandings(
  group: BracketMiniGroup,
  matches: BracketMatch[],
  teams: TeamPublic[],
): MiniGroupStanding[] {
  const teamById = new Map(teams.map((team) => [team.id, team]))
  const matchById = new Map(matches.map((match) => [match.id, match]))
  const groupMatches = group.match_ids
    .map((id) => matchById.get(id))
    .filter((match): match is BracketMatch => Boolean(match))

  return group.team_ids.map((teamId) => {
    let wins = 0
    let losses = 0
    let played = 0
    for (const match of groupMatches) {
      if (match.winner_id === null) continue
      if (match.team1_id === teamId || match.team2_id === teamId) {
        played += 1
        if (match.winner_id === teamId) wins += 1
        else losses += 1
      }
    }
    return {
      teamId,
      team: teamById.get(teamId) ?? null,
      wins,
      losses,
      played,
    }
  })
}

export default function MiniGroupPanel({ miniGroups, matches, teams }: MiniGroupPanelProps) {
  if (miniGroups.length === 0) return null

  return (
    <Card className="p-5 space-y-4">
      <header className="flex items-start gap-2">
        <Users2 size={18} className="mt-0.5 shrink-0 text-primary" />
        <div>
          <h3 className="text-sm font-semibold text-foreground">
            Mini-Round-Robin Slots
          </h3>
          <p className="mt-1 text-xs text-muted">
            Wenn die Teamzahl nicht 1:1 aufteilbar ist, spielen die Teams in einem Slot jeder gegen
            jeden — der Sieger rückt in die nächste Bracket-Runde auf. Kein Team bekommt ein Freilos.
          </p>
        </div>
      </header>

      <div className="grid gap-3 md:grid-cols-2">
        {miniGroups.map((group, index) => {
          const standings = calcStandings(group, matches, teams)
          const sorted = [...standings].sort((a, b) => {
            if (b.wins !== a.wins) return b.wins - a.wins
            return a.losses - b.losses
          })
          const completed = standings.every((entry) => entry.played === group.team_ids.length - 1)

          return (
            <motion.div
              key={group.id}
              initial={{ opacity: 0, y: 8 }}
              animate={{ opacity: 1, y: 0 }}
              transition={{ delay: index * 0.05 }}
              className="rounded-lg border border-border bg-card-hover/40 p-3"
            >
              <div className="mb-2 flex items-center justify-between text-xs">
                <span className="font-medium text-foreground">
                  Runde {group.round} · Slot {group.position + 1}
                </span>
                <span
                  className={`rounded-full px-2 py-0.5 text-[10px] font-medium ${
                    completed
                      ? 'bg-success/15 text-success'
                      : 'bg-muted/15 text-muted'
                  }`}
                >
                  {completed ? 'abgeschlossen' : `${group.team_ids.length} Teams`}
                </span>
              </div>

              <ul className="space-y-1.5">
                {sorted.map((entry, position) => (
                  <li
                    key={entry.teamId}
                    className={`flex items-center justify-between rounded px-2 py-1.5 text-xs ${
                      completed && position === 0
                        ? 'bg-success/10 font-semibold text-success'
                        : 'text-foreground'
                    }`}
                  >
                    <span className="flex min-w-0 items-center gap-2">
                      <span className="w-4 text-[10px] text-muted">{position + 1}.</span>
                      <TeamRosterTooltip
                        teamName={entry.team?.name ?? `Team ${entry.teamId}`}
                        members={entry.team?.members ?? []}
                        align="left"
                      >
                        <span className="cursor-help truncate underline-offset-2 hover:underline">
                          {entry.team?.name ?? `Team ${entry.teamId}`}
                        </span>
                      </TeamRosterTooltip>
                    </span>
                    <span className="ml-2 shrink-0 text-[10px] text-muted">
                      {entry.wins}S / {entry.losses}N
                    </span>
                  </li>
                ))}
              </ul>

              {group.advances_to_match_id && (
                <div className="mt-3 flex items-center gap-1.5 border-t border-border/60 pt-2 text-[10px] text-muted">
                  <ArrowRight size={10} />
                  Sieger rückt in nächste Runde auf
                </div>
              )}
            </motion.div>
          )
        })}
      </div>
    </Card>
  )
}
