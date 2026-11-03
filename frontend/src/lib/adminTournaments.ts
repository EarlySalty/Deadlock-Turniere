import type { AdminTournamentSummary } from '@/types/tournament'

export function tournamentStart(t: AdminTournamentSummary): string | null {
  return t.group_phase_start ?? t.bracket_start ?? null
}

function startValue(t: AdminTournamentSummary): number {
  const start = tournamentStart(t) ?? t.checkin_start ?? t.registration_end
  return start ? new Date(start).getTime() : Number.POSITIVE_INFINITY
}

export function sortByStart(list: AdminTournamentSummary[]): AdminTournamentSummary[] {
  return [...list].sort((a, b) => startValue(a) - startValue(b) || a.id - b.id)
}
