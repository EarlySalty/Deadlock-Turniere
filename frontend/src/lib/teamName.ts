export function teamLabel(name: string | null | undefined, teamId: number): string {
  return name?.trim() || `Team #${teamId}`
}

export function teamNameById(teams: { id: number; name: string }[], teamId: number): string {
  return teamLabel(teams.find((team) => team.id === teamId)?.name, teamId)
}
