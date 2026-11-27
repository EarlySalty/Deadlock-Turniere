import { isLeague, plannedTournamentStart } from './league.ts'
import type { Tournament } from '@/types/tournament'

const OPEN_STATUSES = new Set(['registration', 'checkin', 'group_phase', 'bracket'])
const FINISHED_STATUSES = new Set(['completed', 'archived'])
const STALE_AFTER_MS = 72 * 60 * 60 * 1000

function time(value: string | null | undefined): number {
  return value ? Date.parse(value) : Number.NaN
}

function startTime(t: Tournament): number {
  const start = time(plannedTournamentStart(t))
  return Number.isFinite(start) ? start : Number.POSITIVE_INFINITY
}

function byStart(a: Tournament, b: Tournament): number {
  return startTime(a) - startTime(b) || a.id - b.id
}

export function isOverdueOpenTournament(t: Tournament, now = Date.now()): boolean {
  if (!OPEN_STATUSES.has(t.status) || isLeague(t.tournament_mode)) return false
  const start = startTime(t)
  const lastActivity = time(t.updated_at)
  return (
    Number.isFinite(start) &&
    Number.isFinite(lastActivity) &&
    now - start > STALE_AFTER_MS &&
    now - lastActivity > STALE_AFTER_MS
  )
}

export function groupHomeTournaments(tournaments: Tournament[], now = Date.now()) {
  const overdue = tournaments.filter((t) => isOverdueOpenTournament(t, now))
  const open = tournaments
    .filter((t) => OPEN_STATUSES.has(t.status) && !isOverdueOpenTournament(t, now))
    .sort(byStart)
  const past = tournaments
    .filter((t) => FINISHED_STATUSES.has(t.status) || isOverdueOpenTournament(t, now))
    .sort((a, b) => time(b.updated_at) - time(a.updated_at) || b.id - a.id)
  return { open, past, overdueIds: new Set(overdue.map((t) => t.id)) }
}
