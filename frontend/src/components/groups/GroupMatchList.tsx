import { motion } from 'framer-motion'
import { Swords, Clock, CheckCircle2 } from 'lucide-react'
import Card from '@/components/ui/Card'
import type { Group, Team } from '@/types/tournament'

interface Props {
  groups: Group[]
  teams: Team[]
}

function getTeamName(teamId: number, teams: Team[], groupTeams: Group['teams']): string {
  const groupTeam = groupTeams.find(t => t.team_id === teamId)
  if (groupTeam) return groupTeam.team_name
  const team = teams.find(t => t.id === teamId)
  return team?.name ?? `Team #${teamId}`
}

export default function GroupMatchList({ groups, teams }: Props) {
  const hasMatches = groups.some(g => g.matches && g.matches.length > 0)

  if (!hasMatches) {
    return (
      <Card className="p-6 text-center">
        <Swords size={32} className="mx-auto text-muted mb-3" />
        <p className="text-muted">Noch keine Gruppenspiele geplant</p>
      </Card>
    )
  }

  return (
    <div className="space-y-4">
      <h2 className="text-lg font-semibold text-foreground">Gruppenspiele</h2>
      <div className="grid grid-cols-1 lg:grid-cols-2 gap-4">
        {groups.map((group, groupIndex) => {
          if (!group.matches || group.matches.length === 0) return null

          return (
            <motion.div
              key={group.id}
              initial={{ opacity: 0, y: 20 }}
              animate={{ opacity: 1, y: 0 }}
              transition={{ delay: groupIndex * 0.1 }}
            >
              <Card className="p-0 overflow-hidden">
                <div className="px-5 py-3 border-b border-border flex items-center gap-2">
                  <Swords size={16} className="text-primary" />
                  <h3 className="font-semibold text-foreground">{group.name}</h3>
                  <span className="text-xs text-muted ml-auto">
                    {group.matches.filter(m => m.status === 'completed' || m.status === 'forfeit').length}/{group.matches.length} gespielt
                  </span>
                </div>
                <div className="divide-y divide-border/50">
                  {group.matches.map(match => {
                    const isCompleted = match.status === 'completed' || match.status === 'forfeit'
                    const team1Name = getTeamName(match.team1_id, teams, group.teams)
                    const team2Name = getTeamName(match.team2_id, teams, group.teams)
                    const team1Won = match.winner_id === match.team1_id
                    const team2Won = match.winner_id === match.team2_id

                    return (
                      <div key={match.id} className="px-5 py-3">
                        <div className="flex items-center gap-3">
                          <div className="flex-1 text-right">
                            <span
                              className={`text-sm ${
                                team1Won
                                  ? 'font-semibold text-success'
                                  : isCompleted && !team1Won
                                    ? 'text-muted'
                                    : 'text-foreground'
                              }`}
                            >
                              {team1Name}
                            </span>
                          </div>
                          <div className="flex-shrink-0 w-16 text-center">
                            {isCompleted ? (
                              <CheckCircle2 size={14} className="inline text-success" />
                            ) : (
                              <span className="text-xs text-muted font-medium">vs</span>
                            )}
                          </div>
                          <div className="flex-1 text-left">
                            <span
                              className={`text-sm ${
                                team2Won
                                  ? 'font-semibold text-success'
                                  : isCompleted && !team2Won
                                    ? 'text-muted'
                                    : 'text-foreground'
                              }`}
                            >
                              {team2Name}
                            </span>
                          </div>
                          <div className="flex-shrink-0">
                            {!isCompleted && (
                              <span className="inline-flex items-center gap-1 text-xs text-muted">
                                <Clock size={12} />
                                Ausstehend
                              </span>
                            )}
                          </div>
                        </div>
                        {match.scheduled_at && (
                          <p className="text-xs text-muted mt-1 text-center">
                            {new Date(match.scheduled_at).toLocaleDateString('de-DE', {
                              day: '2-digit',
                              month: '2-digit',
                              hour: '2-digit',
                              minute: '2-digit',
                            })}
                          </p>
                        )}
                      </div>
                    )
                  })}
                </div>
              </Card>
            </motion.div>
          )
        })}
      </div>
    </div>
  )
}
