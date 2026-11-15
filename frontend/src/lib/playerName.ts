export function playerName(name: string | null | undefined): string {
  return name?.trim() || 'Unbekannt'
}
