import type { TournamentStatus } from '@/types/tournament'

export type AdminPhase =
  | 'setup'
  | 'participants'
  | 'checkin'
  | 'group_phase'
  | 'bracket'
  | 'voice'

export function defaultPhaseFor(
  status: TournamentStatus,
  hasGroups: boolean,
  hasBracket: boolean,
): AdminPhase {
  if (status === 'draft' || status === 'registration') return 'participants'
  if (status === 'checkin') return 'checkin'
  if (status === 'group_phase') return hasGroups ? 'group_phase' : 'participants'
  if (status === 'bracket') return hasBracket ? 'bracket' : 'participants'
  if (status === 'completed' || status === 'archived') {
    if (hasBracket) return 'bracket'
    if (hasGroups) return 'group_phase'
  }
  return 'setup'
}
