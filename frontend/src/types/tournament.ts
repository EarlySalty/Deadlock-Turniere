export type TournamentStatus = 'draft' | 'registration' | 'group_phase' | 'bracket' | 'completed' | 'archived'
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
  bracket_format: BracketFormat
  created_at: string
}

export interface TournamentDetail extends Tournament {
  teams: Team[]
  groups: Group[]
  bracket_matches: BracketMatch[]
}

export interface Team {
  id: number
  tournament_id: number
  name: string
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
  bracket_type: 'winners' | 'losers' | 'final'
  team1_id: number | null
  team2_id: number | null
  winner_id: number | null
  status: MatchStatus
  party_code: string | null
  scheduled_at: string | null
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
}

export interface ManualResult {
  winner_id: number
}
