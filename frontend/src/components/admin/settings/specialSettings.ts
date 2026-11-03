import type { InviteMode } from '@/types/tournament'

export interface SpecialSettingsState {
  inviteMode: InviteMode
  inviteWindowStart: string
  inviteWindowEnd: string
  noShowGrace: number
  excludeFromLeaderboard: boolean
}

export function defaultSpecialSettings(): SpecialSettingsState {
  return {
    inviteMode: 'always',
    inviteWindowStart: '',
    inviteWindowEnd: '',
    noShowGrace: 10,
    excludeFromLeaderboard: false,
  }
}
