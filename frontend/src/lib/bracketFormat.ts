import type { BracketFormat, BracketMatch } from '../types/tournament'

export const DOUBLE_ELIMINATION_RULE = 'mindestens 4 Teams, und die Zahl muss eine Zweierpotenz sein (4, 8, 16, 32 …)'

export function hasDoubleEliminationMatches(matches: Pick<BracketMatch, 'bracket_type'>[]): boolean {
  return matches.some((match) => match.bracket_type === 'losers' || match.bracket_type === 'grand_final')
}

export function effectiveBracketFormat(tournament: {
  bracket_format: BracketFormat
  played_bracket_format?: BracketFormat | null
}): BracketFormat {
  return tournament.played_bracket_format ?? tournament.bracket_format
}

export function bracketFormatLabel(format: BracketFormat): string {
  return format === 'single_elimination' ? 'Single Elim.' : 'Double Elim.'
}
