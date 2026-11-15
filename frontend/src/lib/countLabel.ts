export function pluralWord(count: number, singular: string, plural: string): string {
  return count === 1 ? singular : plural
}

export function pluralize(count: number, singular: string, plural: string): string {
  return `${count} ${pluralWord(count, singular, plural)}`
}

export function registrationSummary(teamCount: number, soloCount: number): string {
  if (teamCount === 0) return `${soloCount} Spieler angemeldet`
  const teams = pluralize(teamCount, 'Team', 'Teams')
  return soloCount > 0 ? `${teams}, ${soloCount} Spieler solo` : `${teams} gemeldet`
}
