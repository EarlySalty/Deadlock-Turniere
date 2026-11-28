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

function distanceFrom(now: number) {
  return (t: Tournament) => Math.abs(startTime(t) - now)
}

export function groupHomeTournaments(tournaments: Tournament[], now: number = Date.now()) {
  const distance = distanceFrom(now)
  const open = tournaments
    .filter((t) => OPEN_STATUSES.has(t.status))
    .sort((a, b) => distance(a) - distance(b) || a.id - b.id)
  const past = tournaments
    .filter((t) => FINISHED_STATUSES.has(t.status))
    .sort((a, b) => time(b.updated_at) - time(a.updated_at) || b.id - a.id)
  return { open, past }
}
