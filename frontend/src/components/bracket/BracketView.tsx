import { motion } from 'framer-motion'
import { GitBranch } from 'lucide-react'
import Card from '@/components/ui/Card'
import BracketMatch from './BracketMatch'
import type { BracketMatch as BracketMatchType, Team } from '@/types/tournament'

interface Props {
  matches: BracketMatchType[]
  teams: Team[]
}

function getRoundLabel(round: number, maxRound: number): string {
  if (round === maxRound) return 'Finale'
  if (round === maxRound - 1) return 'Halbfinale'
  if (round === maxRound - 2) return 'Viertelfinale'
  return `Runde ${round}`
}

export default function BracketView({ matches, teams }: Props) {
  if (matches.length === 0) {
    return (
      <Card className="p-6 text-center">
        <GitBranch size={32} className="mx-auto text-muted mb-3" />
        <p className="text-muted">Bracket noch nicht generiert</p>
      </Card>
    )
  }

  // Group matches by round
  const roundsMap = new Map<number, BracketMatchType[]>()
  for (const match of matches) {
    const existing = roundsMap.get(match.round) ?? []
    existing.push(match)
    roundsMap.set(match.round, existing)
  }

  // Sort rounds and matches within rounds by position
  const rounds = Array.from(roundsMap.entries())
    .sort(([a], [b]) => a - b)
    .map(([round, roundMatches]) => ({
      round,
      matches: roundMatches.sort((a, b) => a.position - b.position),
    }))

  const maxRound = Math.max(...rounds.map(r => r.round))

  return (
    <div className="space-y-4">
      <div className="overflow-x-auto pb-4">
        <div className="inline-flex gap-0 min-w-max">
          {rounds.map((roundData, roundIndex) => (
            <div key={roundData.round} className="flex flex-col items-center">
              {/* Round header */}
              <motion.div
                initial={{ opacity: 0, y: -10 }}
                animate={{ opacity: 1, y: 0 }}
                transition={{ delay: roundIndex * 0.1 }}
                className="mb-4"
              >
                <span className="text-xs font-semibold text-muted uppercase tracking-wider">
                  {getRoundLabel(roundData.round, maxRound)}
                </span>
              </motion.div>

              {/* Matches column with connectors */}
              <div className="flex flex-col justify-around flex-1 gap-4 relative">
                {roundData.matches.map((match, matchIndex) => {
                  // Calculate spacing to align with previous round pairs
                  const spacingMultiplier = Math.pow(2, roundIndex)
                  const topPadding = matchIndex === 0
                    ? (spacingMultiplier - 1) * 32
                    : 0
                  const gap = roundIndex > 0
                    ? (spacingMultiplier - 1) * 64
                    : 0

                  return (
                    <div
                      key={match.id}
                      className="flex items-center"
                      style={{
                        marginTop: matchIndex === 0 ? topPadding : gap,
                      }}
                    >
                      {/* Connector line from left */}
                      {roundIndex > 0 && (
                        <div className="w-6 border-t-2 border-border/40" />
                      )}

                      <BracketMatch match={match} teams={teams} />

                      {/* Connector line to right */}
                      {roundData.round < maxRound && (
                        <div className="w-6 border-t-2 border-border/40" />
                      )}
                    </div>
                  )
                })}
              </div>
            </div>
          ))}
        </div>
      </div>
    </div>
  )
}
