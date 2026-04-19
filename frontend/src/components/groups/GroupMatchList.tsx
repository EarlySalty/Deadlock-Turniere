import { motion } from 'framer-motion'
import { CheckCircle2, Clock, KeyRound, Radio, Swords } from 'lucide-react'
import Card from '@/components/ui/Card'
import type { Group, GroupMatch, TeamPublic } from '@/types/tournament'

interface Props {
  groups: Group[]
  teams: TeamPublic[]
}

interface RoundEntry {
  round: number
  matches: GroupMatch[]
}

function getTeamName(teamId: number, teams: TeamPublic[], groupTeams: Group['teams']): string {
  const groupTeam = groupTeams.find((team) => team.team_id === teamId)
  if (groupTeam) return groupTeam.team_name
  const team = teams.find((entry) => entry.id === teamId)
  return team?.name ?? `Team #${teamId}`
}

function getPairKey(teamA: number, teamB: number): string {
  return [teamA, teamB].sort((left, right) => left - right).join(':')
}

function buildRoundPairs(teamIds: number[]): Array<Array<[number, number]>> {
  const ids = [...teamIds]
  const hasBye = ids.length % 2 === 1
  if (hasBye) ids.push(-1)

  const rounds: Array<Array<[number, number]>> = []
  const working = [...ids]
  const roundsCount = Math.max(working.length - 1, 0)
  const half = working.length / 2

  for (let round = 0; round < roundsCount; round += 1) {
    const pairs: Array<[number, number]> = []
    for (let index = 0; index < half; index += 1) {
      const team1 = working[index]
      const team2 = working[working.length - 1 - index]
      if (team1 !== -1 && team2 !== -1) {
        pairs.push([team1, team2])
      }
    }
    rounds.push(pairs)
    const fixed = working[0]
    const rotated = [fixed, working[working.length - 1], ...working.slice(1, working.length - 1)]
    working.splice(0, working.length, ...rotated)
  }

  return rounds
}

function buildRounds(group: Group): RoundEntry[] {
  const pairToMatch = new Map(group.matches.map((match) => [getPairKey(match.team1_id, match.team2_id), match]))
  const roundPairs = buildRoundPairs(group.teams.map((team) => team.team_id))
  const usedMatchIds = new Set<number>()
  const rounds: RoundEntry[] = []

  roundPairs.forEach((pairs, index) => {
    const matches = pairs
      .map(([team1, team2]) => pairToMatch.get(getPairKey(team1, team2)) ?? null)
      .filter((match): match is GroupMatch => match !== null)
    matches.forEach((match) => usedMatchIds.add(match.id))
    if (matches.length > 0) {
      rounds.push({ round: index + 1, matches })
    }
  })

  const leftovers = group.matches
    .filter((match) => !usedMatchIds.has(match.id))
    .sort((left, right) => left.id - right.id)

  if (leftovers.length > 0) {
    rounds.push({ round: rounds.length + 1, matches: leftovers })
  }

  return rounds
}

function statusChip(match: GroupMatch) {
  if (match.status === 'completed' || match.status === 'forfeit') {
    return (
      <span className="inline-flex items-center gap-1 rounded-full border border-green-500/20 bg-green-500/10 px-2 py-1 text-[11px] font-medium text-green-400">
        <CheckCircle2 size={12} />
        Fertig
      </span>
    )
  }
  if (match.status === 'lobby_created') {
    return (
      <span className="inline-flex items-center gap-1 rounded-full border border-sky-500/20 bg-sky-500/10 px-2 py-1 text-[11px] font-medium text-sky-300">
        <KeyRound size={12} />
        Lobby offen
      </span>
    )
  }
  if (match.status === 'in_progress') {
    return (
      <span className="inline-flex items-center gap-1 rounded-full border border-amber-500/20 bg-amber-500/10 px-2 py-1 text-[11px] font-medium text-amber-300">
        <Radio size={12} />
        Läuft
      </span>
    )
  }
  return (
    <span className="inline-flex items-center gap-1 rounded-full border border-border bg-background px-2 py-1 text-[11px] font-medium text-muted">
      <Clock size={12} />
      Ausstehend
    </span>
  )
}

function formatSchedule(value: string | null): string {
  if (!value) return 'Zeit folgt'
  return new Date(value).toLocaleString('de-DE', {
    day: '2-digit',
    month: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
  })
}

export default function GroupMatchList({ groups, teams }: Props) {
  const groupsWithMatches = groups
    .map((group) => ({ ...group, rounds: buildRounds(group) }))
    .filter((group) => group.rounds.length > 0)

  if (groupsWithMatches.length === 0) {
    return (
      <Card className="p-6 text-center">
        <Swords size={32} className="mx-auto text-muted mb-3" />
        <p className="text-muted">Noch keine Gruppenspiele geplant</p>
      </Card>
    )
  }

  return (
    <div className="space-y-4">
      <div>
        <h2 className="text-lg font-semibold text-foreground">Spielplan Gruppenphase</h2>
        <p className="text-sm text-muted">
          Runde für Runde sehen, welches Team gegeneinander spielt und welcher Status gerade aktiv ist.
        </p>
      </div>

      <div className="grid grid-cols-1 gap-4 xl:grid-cols-2">
        {groupsWithMatches.map((group, groupIndex) => (
          <motion.div
            key={group.id}
            initial={{ opacity: 0, y: 20 }}
            animate={{ opacity: 1, y: 0 }}
            transition={{ delay: groupIndex * 0.08 }}
          >
            <Card className="overflow-hidden p-0">
              <div className="border-b border-border px-5 py-4">
                <div className="flex items-center gap-2">
                  <Swords size={16} className="text-primary" />
                  <h3 className="font-semibold text-foreground">{group.name}</h3>
                  <span className="ml-auto text-xs text-muted">
                    {group.matches.filter((match) => match.status === 'completed' || match.status === 'forfeit').length}/{group.matches.length} fertig
                  </span>
                </div>
              </div>

              <div className="space-y-4 p-4">
                {group.rounds.map((roundEntry) => (
                  <div key={`${group.id}-${roundEntry.round}`} className="rounded-xl border border-border bg-background/50 p-4">
                    <div className="mb-3 flex items-center justify-between gap-3">
                      <div>
                        <div className="text-xs uppercase tracking-wider text-muted">Runde {roundEntry.round}</div>
                        <div className="text-sm font-medium text-foreground">
                          {roundEntry.matches.length} Match{roundEntry.matches.length === 1 ? '' : 'es'}
                        </div>
                      </div>
                    </div>

                    <div className="space-y-3">
                      {roundEntry.matches.map((match, matchIndex) => {
                        const team1Name = getTeamName(match.team1_id, teams, group.teams)
                        const team2Name = getTeamName(match.team2_id, teams, group.teams)
                        const team1Won = match.winner_id === match.team1_id
                        const team2Won = match.winner_id === match.team2_id

                        return (
                          <div key={match.id} className="rounded-lg border border-border/70 bg-background px-4 py-3">
                            <div className="flex flex-col gap-3 lg:flex-row lg:items-center lg:justify-between">
                              <div className="space-y-2">
                                <div className="text-xs uppercase tracking-wider text-muted">
                                  Spiel {matchIndex + 1}
                                </div>
                                <div className="flex items-center gap-3 text-sm">
                                  <span className={team1Won ? 'font-semibold text-green-400' : 'text-foreground'}>
                                    {team1Name}
                                  </span>
                                  <span className="text-muted">vs</span>
                                  <span className={team2Won ? 'font-semibold text-green-400' : 'text-foreground'}>
                                    {team2Name}
                                  </span>
                                </div>
                                <div className="text-xs text-muted">
                                  {formatSchedule(match.scheduled_at)}
                                </div>
                              </div>

                              <div className="flex flex-wrap items-center gap-2">
                                {statusChip(match)}
                                {match.party_code && (
                                  <span className="rounded-full border border-primary/20 bg-primary/10 px-2 py-1 text-[11px] font-medium text-primary">
                                    Code {match.party_code}
                                  </span>
                                )}
                              </div>
                            </div>
                          </div>
                        )
                      })}
                    </div>
                  </div>
                ))}
              </div>
            </Card>
          </motion.div>
        ))}
      </div>
    </div>
  )
}
