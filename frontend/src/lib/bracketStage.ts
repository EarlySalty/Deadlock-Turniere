import type { BracketMatch } from '../types/tournament'

type StageMatch = Pick<BracketMatch, 'round' | 'bracket_type'> & Partial<Pick<BracketMatch, 'mini_group_id'>>

function roundsOf(matches: StageMatch[], bracketType: BracketMatch['bracket_type']): number[] {
  return matches.filter((m) => m.bracket_type === bracketType).map((m) => m.round)
}

function winnersLabel(round: number, maxRound: number): string {
  if (round === maxRound) return 'Finale'
  if (round === maxRound - 1) return 'Halbfinale'
  if (round === maxRound - 2) return 'Viertelfinale'
  return `Runde ${round}`
}

function inGroup(match: StageMatch): boolean {
  return match.mini_group_id !== null && match.mini_group_id !== undefined
}

function winnersInRound(matches: StageMatch[], round: number): StageMatch[] {
  return matches.filter((m) => m.bracket_type === 'winners' && m.round === round)
}

function advancersOf(matches: StageMatch[], round: number): number {
  const inRound = winnersInRound(matches, round)
  const groups = new Set(inRound.filter(inGroup).map((m) => m.mini_group_id))
  return inRound.filter((m) => !inGroup(m)).length + groups.size
}

function miniGroupLabel(round: number, maxRound: number, matches: StageMatch[]): string {
  const finalIsGroup = winnersInRound(matches, maxRound).some(inGroup)
  if (round === maxRound) return finalIsGroup ? 'Finalgruppe' : 'Finale'
  if (finalIsGroup) return `Runde ${round}`
  const advancers = advancersOf(matches, round)
  if (advancers === 2) return 'Halbfinale'
  if (advancers === 4) return 'Viertelfinale'
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
  const maxRound = Math.max(...roundsOf(matches, 'winners'))
  const hasMiniGroups = matches.some((m) => m.bracket_type === 'winners' && inGroup(m))
  const label = hasMiniGroups ? miniGroupLabel(match.round, maxRound, matches) : winnersLabel(match.round, maxRound)
  return isDoubleElimination(matches) ? `WB-${label}` : label
}
