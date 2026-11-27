import type { AdminTournamentSummary } from '@/types/tournament'
import { isOverdueOpenTournament } from './homeTournaments.ts'

export function tournamentStart(t: AdminTournamentSummary): string | null {
  const planned = t.tournament_mode === 'bracket_only'
    ? t.bracket_start
    : t.group_phase_start ?? t.bracket_start
  return planned ?? t.registration_end
}

function startValue(t: AdminTournamentSummary): number {
  const start = tournamentStart(t) ?? t.checkin_start ?? t.registration_end
  return start ? new Date(start).getTime() : Number.POSITIVE_INFINITY
}

export function sortByStart(list: AdminTournamentSummary[]): AdminTournamentSummary[] {
  return [...list].sort((a, b) => startValue(a) - startValue(b) || a.id - b.id)
}
export function sortAdminLive(list: AdminTournamentSummary[], now = Date.now()): AdminTournamentSummary[] {
  return [...list].sort((a, b) =>
    Number(isOverdueOpenTournament(a, now)) - Number(isOverdueOpenTournament(b, now))
    || startValue(a) - startValue(b)
    || a.id - b.id,
  )
}
