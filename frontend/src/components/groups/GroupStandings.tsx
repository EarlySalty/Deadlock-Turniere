import { motion } from 'framer-motion'
import { Trophy, LayoutGrid } from 'lucide-react'
import Card from '@/components/ui/Card'
import TeamRosterTooltip from '@/components/ui/TeamRosterTooltip'
import { LEAGUE_SCORING_TEXT } from '@/lib/league'
import type { Group, TeamPublic } from '@/types/tournament'

const QUALIFIERS_PER_GROUP = 2

interface Props {
  groups: Group[]
  teams?: TeamPublic[]
  league?: boolean
  admin?: boolean
}

export default function GroupStandings({ groups, teams = [], league = false, admin = false }: Props) {
  if (groups.length === 0) {
    return (
      <Card className="p-6 text-center">
        <LayoutGrid size={32} className="mx-auto text-muted mb-3" />
        <p className="text-muted">
          {league
            ? 'Die Tabelle erscheint, sobald der Check-in abgeschlossen ist und der Spielplan steht.'
            : 'Gruppenphase noch nicht gestartet'}
        </p>
      </Card>
    )
  }

  const teamById = new Map<number, TeamPublic>()
  for (const team of teams) {
    teamById.set(team.id, team)
  }

  return (
    <div className={`grid grid-cols-1 gap-4 ${league ? '' : 'lg:grid-cols-2'}`}>
      {groups.map((group, groupIndex) => {
        return (
          <motion.div
            key={group.id}
            initial={{ opacity: 0, y: 20 }}
            animate={{ opacity: 1, y: 0 }}
            transition={{ delay: groupIndex * 0.1 }}
          >
            <Card className="p-0 overflow-hidden">
              <div className="px-5 py-3 border-b border-border flex items-center gap-2">
                <Trophy size={16} className="text-primary" />
                <h3 className="font-semibold text-foreground">{group.name}</h3>
              </div>
              <div className="overflow-x-auto">
                <table className="w-full text-sm">
                  <thead>
                    <tr className="border-b border-border text-muted">
                      <th className="text-left px-4 py-2 w-10">#</th>
                      <th className="text-left px-4 py-2">Team</th>
                      <th className="text-center px-4 py-2"><abbr title="Siege" className="no-underline">S</abbr></th>
                      <th className="text-center px-4 py-2"><abbr title="Niederlagen" className="no-underline">N</abbr></th>
                      <th className="text-center px-4 py-2"><abbr title="Punkte" className="no-underline">Pkt</abbr></th>
                    </tr>
                  </thead>
                  <tbody>
                    {group.teams.map((team, index) => {
                      const isQualified = !league && index < QUALIFIERS_PER_GROUP
                      const fullTeam = teamById.get(team.team_id)
                      return (
                        <tr
                          key={team.team_id}
                          className={`border-b border-border/50 last:border-0 ${
                            isQualified ? 'bg-success/10' : ''
                          }`}
                        >
                          <td className="px-4 py-2.5">
                            <span
                              className={`text-xs font-medium ${
                                isQualified ? 'text-success' : 'text-muted'
                              }`}
                            >
                              {index + 1}
                            </span>
                          </td>
                          <td className="px-4 py-2.5">
                            <TeamRosterTooltip
                              teamName={team.team_name}
                              members={fullTeam?.members ?? []}
                              align="left"
                            >
                              <span
                                className={`cursor-help underline-offset-2 hover:underline ${
                                  isQualified
                                    ? 'font-semibold text-foreground'
                                    : 'text-foreground'
                                }`}
                              >
                                {team.team_name}
                              </span>
                            </TeamRosterTooltip>
                            {admin && team.seed_tiebreak && (
                              <span className="mt-0.5 block text-xs text-amber-200">
                                Gleichstand, Reihenfolge nach Setzliste
                              </span>
                            )}
                          </td>
                          <td className="text-center px-4 py-2.5 text-muted">
                            {team.wins}
                          </td>
                          <td className="text-center px-4 py-2.5 text-muted">
                            {team.losses}
                          </td>
                          <td className="text-center px-4 py-2.5">
                            <span
                              className={`font-semibold ${
                                isQualified ? 'text-success' : league ? 'text-foreground' : 'text-muted'
                              }`}
                            >
                              {team.points}
                            </span>
                          </td>
                        </tr>
                      )
                    })}
                  </tbody>
                </table>
              </div>
              {league ? (
                <p className="border-t border-border px-5 py-3 text-xs text-muted">{LEAGUE_SCORING_TEXT}</p>
              ) : (
                <p className="flex items-center gap-2 border-t border-border px-5 py-3 text-xs text-muted">
                  <span aria-hidden="true" className="h-3 w-3 shrink-0 rounded-sm border border-success/40 bg-success/10" />
                  Die besten {QUALIFIERS_PER_GROUP} jeder Gruppe ziehen ins Bracket ein.
                </p>
              )}
            </Card>
          </motion.div>
        )
      })}
    </div>
  )
}
