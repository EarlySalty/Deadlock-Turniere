import type { FinalizeCheckinResult } from '@/types/tournament'

export function keepsRosters(preview: Pick<FinalizeCheckinResult, 'can_fill_from_solos' | 'auto_team_formation'>): boolean {
  return preview.can_fill_from_solos && !preview.auto_team_formation
}
