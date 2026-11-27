import { plannedTournamentStart } from './league.ts'
import type { Tournament } from '@/types/tournament'

const OPEN_STATUSES = new Set(['registration', 'checkin', 'group_phase', 'bracket'])
const FINISHED_STATUSES = new Set(['completed', 'archived'])

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

export function groupHomeTournaments(tournaments: Tournament[]) {
  const open = tournaments.filter((t) => OPEN_STATUSES.has(t.status)).sort(byStart)
  const past = tournaments
    .filter((t) => FINISHED_STATUSES.has(t.status))
    .sort((a, b) => time(b.updated_at) - time(a.updated_at) || b.id - a.id)
  return { open, past }
}
