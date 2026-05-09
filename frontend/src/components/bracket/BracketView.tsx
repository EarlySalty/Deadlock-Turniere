import { motion } from 'framer-motion'
import { GitBranch } from 'lucide-react'
import Card from '@/components/ui/Card'
import BracketMatch from './BracketMatch'
import type { BracketMatch as BracketMatchType, TeamPublic } from '@/types/tournament'

interface Props {
  matches: BracketMatchType[]
  teams: TeamPublic[]
}

function getWinnerRoundLabel(round: number, maxRound: number): string {
  if (round === maxRound) return 'Finale'
  if (round === maxRound - 1) return 'Halbfinale'
  if (round === maxRound - 2) return 'Viertelfinale'
  return `Runde ${round}`
}

function getLoserRoundLabel(round: number, maxRound: number, minRound: number): string {
  if (round === maxRound) return 'LB Finale'
  const idx = round - minRound + 1
  return `LB Runde ${idx}`
}

function getGrandFinalLabel(round: number, gfRounds: number[]): string {
  const sorted = [...gfRounds].sort((a, b) => a - b)
  if (sorted.length >= 2 && round === sorted[1]) return 'Bracket Reset'
  return 'Grand Final'
}

interface BracketColumnsProps {
  matches: BracketMatchType[]
  teams: TeamPublic[]
  roundLabel: (round: number, maxRound: number, minRound: number) => string
}

function BracketColumns({ matches, teams, roundLabel }: BracketColumnsProps) {
  const roundsMap = new Map<number, BracketMatchType[]>()
  for (const match of matches) {
    const existing = roundsMap.get(match.round) ?? []
    existing.push(match)
    roundsMap.set(match.round, existing)
  }

  const rounds = Array.from(roundsMap.entries())
    .sort(([a], [b]) => a - b)
    .map(([round, roundMatches]) => ({
      round,
      matches: roundMatches.sort((a, b) => a.position - b.position),
    }))

  const maxRound = Math.max(...rounds.map((r) => r.round))
  const minRound = Math.min(...rounds.map((r) => r.round))

  return (
    <div className="overflow-x-auto pb-4">
      <div className="inline-flex gap-0 min-w-max">
        {rounds.map((roundData, roundIndex) => (
          <div key={roundData.round} className="flex flex-col items-center">
            <motion.div
              initial={{ opacity: 0, y: -10 }}
              animate={{ opacity: 1, y: 0 }}
              transition={{ delay: roundIndex * 0.1 }}
              className="mb-4"
            >
              <span className="text-xs font-semibold text-muted uppercase tracking-wider">
                {roundLabel(roundData.round, maxRound, minRound)}
              </span>
            </motion.div>

            <div className="flex flex-col justify-around flex-1 gap-4 relative">
              {roundData.matches.map((match, matchIndex) => {
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
                    {roundIndex > 0 && (
                      <div className="w-6 border-t-2 border-border/40" />
                    )}

                    <BracketMatch match={match} teams={teams} />

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
  )
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

  const winners = matches.filter((m) => m.bracket_type === 'winners')
  const losers = matches.filter((m) => m.bracket_type === 'losers')
  const grandFinal = matches.filter((m) => m.bracket_type === 'grand_final')

  const isDoubleElim = losers.length > 0 || grandFinal.length > 0

  if (!isDoubleElim) {
    return (
      <div className="space-y-4">
        <BracketColumns
          matches={winners.length > 0 ? winners : matches}
          teams={teams}
          roundLabel={getWinnerRoundLabel}
        />
      </div>
    )
  }

  const gfRounds = grandFinal.map((m) => m.round)

  return (
    <div className="space-y-8">
      <section>
        <h3 className="text-sm font-semibold text-foreground uppercase tracking-wider mb-3">
          Winner-Bracket
        </h3>
        <BracketColumns
          matches={winners}
          teams={teams}
          roundLabel={getWinnerRoundLabel}
        />
      </section>

      {losers.length > 0 && (
        <section>
          <h3 className="text-sm font-semibold text-foreground uppercase tracking-wider mb-3">
            Loser-Bracket
          </h3>
          <BracketColumns
            matches={losers}
            teams={teams}
            roundLabel={getLoserRoundLabel}
          />
        </section>
      )}

      {grandFinal.length > 0 && (
        <section>
          <h3 className="text-sm font-semibold text-foreground uppercase tracking-wider mb-3">
            Grand Final
          </h3>
          <BracketColumns
            matches={grandFinal}
            teams={teams}
            roundLabel={(round) => getGrandFinalLabel(round, gfRounds)}
          />
        </section>
      )}
    </div>
  )
}
