import type { BracketFormat } from '../types/tournament'

export function effectiveBracketFormat(tournament: {
  bracket_format: BracketFormat
  played_bracket_format?: BracketFormat | null
}): BracketFormat {
  return tournament.played_bracket_format ?? tournament.bracket_format
}

export function bracketFormatLabel(format: BracketFormat): string {
  return format === 'single_elimination' ? 'Single Elim.' : 'Double Elim.'
}
