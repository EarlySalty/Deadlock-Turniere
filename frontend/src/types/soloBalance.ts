export interface BalancePlayer {
  signup_id: number
  discord_id: string
  discord_name: string | null
  steam_id: string | null
  rank: string | null
  rank_score: number
  score: number
  priority: boolean
  used_median: boolean
}

export interface BalancePreview {
  teams: { name: string; players: BalancePlayer[]; score_sum: number }[]
  waitlist: BalancePlayer[]
  median_score: number
  gap: number
  team_count: number
  team_size: number
  preview_token: string
}

export interface BalanceWaitlistEntry {
  signup_id: number
  discord_name: string | null
  position: number
}
