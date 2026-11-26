import type { Tournament, TournamentStatus } from '@/types/tournament'

type StartFields = Pick<Tournament, 'id' | 'tournament_mode' | 'group_phase_start' | 'bracket_start' | 'registration_end'>

export function effectiveStart(t: StartFields): string | null {
  const planned = t.tournament_mode === 'bracket_only' ? t.bracket_start : t.group_phase_start ?? t.bracket_start
  return planned ?? t.registration_end ?? null
}

function startValue(t: StartFields): number {
  const start = effectiveStart(t)
  return start ? new Date(start).getTime() : Number.POSITIVE_INFINITY
}

export function byEffectiveStart(a: StartFields, b: StartFields): number {
  return startValue(a) - startValue(b) || a.id - b.id
}

export function sortByEffectiveStart<T extends StartFields>(list: T[]): T[] {
  return [...list].sort(byEffectiveStart)
}

const SIGNUP_STATUSES: TournamentStatus[] = ['registration', 'checkin']

export function signupOpen(status: TournamentStatus): boolean {
  return SIGNUP_STATUSES.includes(status)
}
