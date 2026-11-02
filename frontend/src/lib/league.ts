import type { GroupMatch, TournamentMode } from '@/types/tournament'

export const LEAGUE_LABEL = 'Jeder gegen jeden'
export const LEAGUE_SCORING_TEXT =
  'Sieg: +2 Punkte, Niederlage: -1 Punkt. Bei Punktgleichheit entscheidet die Zahl der Siege, danach der direkte Vergleich.'
export const DEFAULT_SLOT_MINUTES = 50

export interface LeagueRound {
  round: number
  scheduledAt: string | null
  matches: GroupMatch[]
  openMatches: number
}

export function isLeague(mode: TournamentMode | string | null | undefined): boolean {
  return mode === 'league'
}

export function isDecided(match: GroupMatch): boolean {
  return match.status === 'completed' || match.status === 'forfeit'
}

export function leagueRounds(matches: GroupMatch[]): LeagueRound[] {
  const byRound = new Map<number, GroupMatch[]>()
  for (const match of matches) {
    const round = match.round ?? 1
    byRound.set(round, [...(byRound.get(round) ?? []), match])
  }
  return [...byRound.entries()]
    .sort(([left], [right]) => left - right)
    .map(([round, roundMatches]) => ({
      round,
      scheduledAt: roundMatches[0]?.scheduled_at ?? null,
      matches: roundMatches,
      openMatches: roundMatches.filter((match) => !isDecided(match)).length,
    }))
}

export function slotMinutesOf(rounds: LeagueRound[]): number {
  const [first, second] = rounds
  if (!first?.scheduledAt || !second?.scheduledAt) return DEFAULT_SLOT_MINUTES
  const minutes = (Date.parse(second.scheduledAt) - Date.parse(first.scheduledAt)) / 60000
  return minutes > 0 ? minutes : DEFAULT_SLOT_MINUTES
}

export function toLocalInput(value: string | null | undefined): string {
  if (!value) return ''
  const date = new Date(value)
  if (Number.isNaN(date.getTime())) return ''
  const pad = (part: number) => String(part).padStart(2, '0')
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}T${pad(date.getHours())}:${pad(date.getMinutes())}`
}

export function localInputToIso(value: string): string | undefined {
  if (!value) return undefined
  const date = new Date(value)
  return Number.isNaN(date.getTime()) ? undefined : date.toISOString()
}

export function formatRoundTime(value: string | null): string {
  if (!value) return 'Zeit folgt'
  return new Date(value).toLocaleString('de-DE', {
    weekday: 'short',
    day: '2-digit',
    month: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
  })
}
