export function playerName(name: string | null | undefined, fallback = 'Unbekannt'): string {
  return name?.trim() || fallback
}
