export type TournamentStatus = 'draft' | 'registration' | 'checkin' | 'group_phase' | 'bracket' | 'completed' | 'archived'
export type BracketFormat = 'single_elimination' | 'double_elimination'
export type MatchStatus = 'pending' | 'checkin' | 'lobby_created' | 'in_progress' | 'completed' | 'forfeit' | 'cancelled'

export interface UserSession {
  discord_id: string
  discord_name: string
  discord_avatar: string | null
  roles: string[]
  is_admin: boolean
  is_mod: boolean
}

export interface Tournament {
  id: number
  name: string
  status: TournamentStatus
  description: string | null
  team_size: number
  registration_start: string | null
  registration_end: string | null
  group_phase_start: string | null
  bracket_start: string | null
  bracket_format: BracketFormat
  created_by: string
  created_at: string
  updated_at: string
}

export interface TournamentDetail extends Tournament {
  teams: Team[]
  groups: Group[]
  bracket_matches: BracketMatch[]
  signups: TournamentSignup[]
}

export interface Team {
  id: number
  tournament_id: number
  name: string
  name_key: string
  captain_discord_id: string
  members: TeamMember[]
  created_at: string
}

export interface TeamMember {
  discord_id: string
  discord_name: string | null
  steam_id: string | null
  rank: string | null
  rank_score: number
  role: 'captain' | 'member'
  joined_at: string
}

export interface TournamentSignup {
  id: number
  tournament_id: number
  discord_id: string
  discord_name: string | null
  steam_id: string | null
  rank: string | null
  rank_score: number
  team_id: number | null
  signed_up_at: string
}

export interface CheckinStatus {
  total_registered: number
  total_checked_in: number
  checked_in_discord_ids: string[]
}

export interface FinalizeCheckinWarning {
  team_id: number
  team_name: string
  current: number
  required: number
}

export interface FinalizeCheckinPlayerChange {
  team_id: number | null
  team_name: string
  discord_id: string
  discord_name: string | null
  source?: 'solo_pool' | 'new_team'
}

export interface FinalizeCheckinResult {
  warnings: FinalizeCheckinWarning[]
  removed_players: FinalizeCheckinPlayerChange[]
  added_players: FinalizeCheckinPlayerChange[]
  created_teams: { team_id: number | null; team_name: string }[]
  deleted_team_ids: number[]
  remaining_solo_players: {
    discord_id: string
    discord_name: string | null
  }[]
  dry_run: boolean
  snapshot_token: string
  groups_created?: number
  matches_created?: number
  advanced_to_group_phase?: boolean
}

export interface GroupMatch {
  id: number
  group_id: number
  team1_id: number
  team2_id: number
  winner_id: number | null
  status: MatchStatus
  scheduled_at: string | null
  played_at: string | null
}

export interface Group {
  id: number
  name: string
  teams: GroupTeam[]
  matches: GroupMatch[]
}

export interface GroupTeam {
  team_id: number
  team_name: string
  wins: number
  losses: number
  points: number
}

export interface BracketMatch {
  id: number
  round: number
  position: number
  bracket_type: 'winners' | 'losers' | 'grand_final'
  team1_id: number | null
  team2_id: number | null
  winner_id: number | null
  status: MatchStatus
  steam_party_id: string | null
  party_code: string | null
  deadlock_match_id: string | null
  match_duration_s: number | null
  match_stats: string | null
  scheduled_at: string | null
  played_at: string | null
}

export interface TournamentCreate {
  name: string
  description?: string
  team_size: number
  bracket_format: BracketFormat
  registration_start?: string
  registration_end?: string
}

export interface TournamentUpdate {
  name?: string
  description?: string
  status?: TournamentStatus
  team_size?: number
  bracket_format?: BracketFormat
  registration_start?: string
  registration_end?: string
  group_phase_start?: string
  bracket_start?: string
}

export interface ManualResult {
  winner_id: number
}

export interface LobbyCreateResult {
  success: boolean
  party_id: string
  party_code: string | null
  join_code: string | null
}

export interface MatchStartResult {
  success: boolean
  match_id: number | string | null
}

export interface MatchPlayerStats {
  account_id: number
  team: number
  hero_id: number
  kills: number
  deaths: number
  assists: number
  net_worth: number
  last_hits: number
}

export interface MatchFetchResult {
  success: boolean
  match_id: number | string
  winner_id: number
  winning_team: number
  duration_s: number | null
  players: MatchPlayerStats[]
}

export interface TeamMoveRequest {
  from_team_id: number
  discord_id: string
}
