import type { BracketFormat, BracketMatch } from '../types/tournament'

export function effectiveBracketFormat(
  storedFormat: BracketFormat,
  matches: Pick<BracketMatch, 'bracket_type'>[] | undefined,
): BracketFormat {
  if (!matches || matches.length === 0) return storedFormat
  const hasDoubleElimMatches = matches.some(
    (match) => match.bracket_type === 'losers' || match.bracket_type === 'grand_final',
  )
  return hasDoubleElimMatches ? 'double_elimination' : 'single_elimination'
}

export function bracketFormatLabel(format: BracketFormat): string {
  return format === 'single_elimination' ? 'Single Elim.' : 'Double Elim.'
}
