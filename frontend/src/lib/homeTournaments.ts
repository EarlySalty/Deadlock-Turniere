import { plannedTournamentStart } from './league.ts'
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

/**
 * Ein Turnier kann in der DB auf bracket/registration stehen bleiben,
 * obwohl sein geplanter Start längst vorbei ist. Das ist kein Abschluss:
 * erst bei ausbleibender Aktivität wird es aus "Aktuelle" ausgeblendet.
 */
export function isOverdueOpenTournament(t: Tournament, now = Date.now()): boolean {
  if (!OPEN_STATUSES.has(t.status)) return false
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
  const visible = tournaments.filter((t) => !t.is_test && t.status !== 'draft')
  const overdue = visible.filter((t) => isOverdueOpenTournament(t, now))
  const open = visible
    .filter((t) => OPEN_STATUSES.has(t.status) && !isOverdueOpenTournament(t, now))
    .sort(byStart)
  const past = visible
    .filter((t) => FINISHED_STATUSES.has(t.status) || isOverdueOpenTournament(t, now))
    .sort((a, b) => time(b.updated_at) - time(a.updated_at) || b.id - a.id)
  return { open, past, overdueIds: new Set(overdue.map((t) => t.id)) }
}
