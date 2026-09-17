import type { CompComposition, CompPreference, CompPriority, CompRoom } from '../types/comp.ts'

export function normalizeCompCode(input: string): string | null {
  const value = input.trim().toUpperCase()
  if (/^[A-HJ-NP-Z2-9]{8}$/.test(value)) return value
  try {
    const url = new URL(input.trim())
    if (!['https:', 'http:'].includes(url.protocol)) return null
    return url.pathname.toUpperCase().match(/\/(?:TURNIER\/)?COMP\/([A-HJ-NP-Z2-9]{8})\/?$/)?.[1] ?? null
  } catch {
    return null
  }
}

export function compTokenKey(code: string): string {
  return `comp-player:${code.toUpperCase()}`
}

export function compShareUrl(origin: string, base: string, code: string): string {
  const normalized = normalizeCompCode(code)
  if (!normalized) throw new Error('Ungültiger Lobby-Code')
  return `${origin}${base.replace(/\/?$/, '/')}comp/${normalized}`
}

export function nextPriority(current: CompPriority | undefined): CompPriority | undefined {
  return current === undefined ? 0 : current === 2 ? undefined : (current + 1) as CompPriority
}

export function priorityLabel(priority: CompPriority | undefined): string {
  return priority === undefined ? 'Nicht ausgewählt' : ['Spielbar', 'Bevorzugt', 'Höchste Priorität'][priority]
}

export function preferenceMap(preferences: CompPreference[]): Record<string, CompPriority> {
  return Object.fromEntries(preferences.map(p => [p.hero_name, p.priority]))
}

export function preferenceList(preferences: Record<string, CompPriority>): CompPreference[] {
  return Object.entries(preferences).sort(([a], [b]) => a.localeCompare(b)).map(([hero_name, priority]) => ({ hero_name, priority }))
}

export function compositionText(room: CompRoom, composition: CompComposition, rank: number): string {
  return [
    `Comp ${rank} – ${composition.score}/${room.results.max_score} Wunschpunkte`,
    ...composition.assignments.map(a => `${room.members[a.player_index]?.name ?? 'Spieler'}: ${a.hero_name} (${a.priority} P.)`),
    'Bewertet Heldenwünsche, keine Meta- oder Synergie-Bewertung.',
  ].join('\n')
}

export function newestRoom(previous: CompRoom | undefined, incoming: CompRoom): CompRoom {
  return previous && previous.code === incoming.code && previous.revision > incoming.revision ? previous : incoming
}
