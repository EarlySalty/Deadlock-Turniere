import { plannedTournamentStart } from './league.ts'
import type { AdminTournamentSummary } from '@/types/tournament'

export function tournamentStart(t: AdminTournamentSummary): string | null {
  return plannedTournamentStart(t)
}

function startValue(t: AdminTournamentSummary): number {
  const start = tournamentStart(t)
  return start ? new Date(start).getTime() : Number.POSITIVE_INFINITY
}

export function sortByStart(list: AdminTournamentSummary[]): AdminTournamentSummary[] {
  return [...list].sort((a, b) => startValue(a) - startValue(b) || a.id - b.id)
}
