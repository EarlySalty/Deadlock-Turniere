import { motion } from 'framer-motion'
import { Trophy, LayoutGrid } from 'lucide-react'
import Card from '@/components/ui/Card'
import type { Group } from '@/types/tournament'

interface Props {
  groups: Group[]
}

export default function GroupStandings({ groups }: Props) {
  if (groups.length === 0) {
    return (
      <Card className="p-6 text-center">
        <LayoutGrid size={32} className="mx-auto text-muted mb-3" />
        <p className="text-muted">Gruppenphase noch nicht gestartet</p>
      </Card>
    )
  }

  return (
    <div className="grid grid-cols-1 lg:grid-cols-2 gap-4">
      {groups.map((group, groupIndex) => {
        const sortedTeams = [...group.teams].sort((a, b) => {
          if (b.points !== a.points) return b.points - a.points
          return b.wins - a.wins
        })

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
                      <th className="text-center px-4 py-2">S</th>
                      <th className="text-center px-4 py-2">N</th>
                      <th className="text-center px-4 py-2">Pkt</th>
                    </tr>
                  </thead>
                  <tbody>
                    {sortedTeams.map((team, index) => {
                      const isQualified = index < 2
                      return (
                        <tr
                          key={team.team_id}
                          className={`border-b border-border/50 last:border-0 ${
                            isQualified ? 'bg-success/5' : ''
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
                            <span
                              className={`${
                                isQualified
                                  ? 'font-semibold text-foreground'
                                  : 'text-foreground'
                              }`}
                            >
                              {team.team_name}
                            </span>
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
                                isQualified ? 'text-success' : 'text-muted'
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
            </Card>
          </motion.div>
        )
      })}
    </div>
  )
}
