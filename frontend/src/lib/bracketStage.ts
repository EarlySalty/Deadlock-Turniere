import type { BracketMatch } from '../types/tournament'

type StageMatch = Pick<BracketMatch, 'round' | 'bracket_type'>

function roundsOf(matches: StageMatch[], bracketType: BracketMatch['bracket_type']): number[] {
  return matches.filter((m) => m.bracket_type === bracketType).map((m) => m.round)
}

function winnersLabel(round: number, maxRound: number): string {
  if (round === maxRound) return 'Finale'
  if (round === maxRound - 1) return 'Halbfinale'
  if (round === maxRound - 2) return 'Viertelfinale'
  return `Runde ${round}`
}

export function isDoubleElimination(matches: StageMatch[]): boolean {
  return matches.some((m) => m.bracket_type === 'losers' || m.bracket_type === 'grand_final')
}

export function bracketStageLabel(match: StageMatch, matches: StageMatch[]): string {
  if (match.bracket_type === 'grand_final') {
    const firstGrandFinal = Math.min(...roundsOf(matches, 'grand_final'))
    return match.round > firstGrandFinal ? 'Grand Final Reset' : 'Grand Final'
  }
  if (match.bracket_type === 'losers') {
    const losersRounds = roundsOf(matches, 'losers')
    const maxRound = Math.max(...losersRounds)
    if (match.round === maxRound) return 'LB-Finale'
    return `LB Runde ${match.round - Math.min(...losersRounds) + 1}`
  }
  const label = winnersLabel(match.round, Math.max(...roundsOf(matches, 'winners')))
  return isDoubleElimination(matches) ? `WB-${label}` : label
}
