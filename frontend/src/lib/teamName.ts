export function teamNameOr(name: string | null | undefined, fallback: string): string {
  return name?.trim() || fallback
}

export function teamLabel(name: string | null | undefined, teamId: number): string {
  return teamNameOr(name, `Team #${teamId}`)
}

export function teamNameById(teams: { id: number; name: string }[], teamId: number): string {
  return teamLabel(teams.find((team) => team.id === teamId)?.name, teamId)
}
