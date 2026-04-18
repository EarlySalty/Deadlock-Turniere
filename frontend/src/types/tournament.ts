export type TournamentStatus = 'draft' | 'registration' | 'checkin' | 'group_phase' | 'bracket' | 'completed' | 'archived'
export type BracketFormat = 'single_elimination' | 'double_elimination'
export type MatchStatus = 'pending' | 'checkin' | 'lobby_created' | 'in_progress' | 'completed' | 'forfeit' | 'cancelled'
export type RecruitmentStatus = 'open' | 'application' | 'closed'
export type InviteMode = 'always' | 'window' | 'never'
export type LobbySettingsPreset =
  | 'standard'
  | 'fast_mode'
  | 'high_damage'
  | 'low_gravity'
  | 'speed_mode'
  | 'glass_cannon'
  | 'rich_start'
  | 'chaos_mode'
  | 'all_same_hero'
  | 'immortal'
  | 'custom'
export type InvitationStatus = 'pending' | 'accepted' | 'rejected' | 'expired'
export type ApplicationStatus = 'pending' | 'accepted' | 'rejected'

export interface UserSession {
  discord_id: string
  discord_name: string
  discord_avatar: string | null
  roles: string[]
  is_admin: boolean
  is_mod: boolean
}

// --- Tournament (base, used in lists and admin) ---

export interface Tournament {
  id: number
  name: string
  status: TournamentStatus
  description: string | null
  team_size: number
  series_format: 1 | 3 | 5
  registration_start: string | null
  registration_end: string | null
  checkin_start: string | null
  group_phase_start: string | null
  bracket_start: string | null
  bracket_format: BracketFormat
  invite_mode: InviteMode
  invite_window_start: string | null
  invite_window_end: string | null
  created_by: string
  created_at: string
  updated_at: string
}

// --- Admin types (with discord_id) ---

export interface TeamMember {
  id?: number
  team_id?: number
  discord_id: string
  discord_name: string | null
  steam_id: string | null
  rank: string | null
  rank_score: number
  role: 'captain' | 'member'
  joined_at: string
}

export interface Team {
  id: number
  tournament_id: number
  name: string
  name_key: string
  captain_discord_id: string
  members: TeamMember[]
  created_at: string
  recruitment_status: RecruitmentStatus
  has_pending_applications: boolean
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

export interface TournamentDetail extends Tournament {
  teams: Team[]
  groups: Group[]
  bracket_matches: BracketMatch[]
  signups: TournamentSignup[]
}

// --- Public types (no discord_id, for /api/tournaments/{id}) ---

export interface TeamMemberPublic {
  id?: number
  team_id?: number
  discord_name: string | null
  steam_id: string | null
  rank: string | null
  rank_score: number
  role: 'captain' | 'member'
  joined_at: string
}

export interface TeamPublic {
  id: number
  tournament_id: number
  name: string
  name_key: string
  members: TeamMemberPublic[]
  created_at: string
  recruitment_status: RecruitmentStatus
  has_pending_applications: boolean
}

export interface TournamentSignupPublic {
  id: number
  tournament_id: number
  discord_name: string | null
  rank: string | null
  rank_score: number
  team_id: number | null
  signed_up_at: string
}

export interface TournamentDetailPublic extends Tournament {
  teams: TeamPublic[]
  groups: Group[]
  bracket_matches: BracketMatch[]
  signups: TournamentSignupPublic[]
}

// --- User-specific tournament status (from /api/tournaments/{id}/me) ---

export interface MyTournamentStatus {
  team_id: number | null
  signup_id: number | null
  is_captain: boolean
  is_checked_in: boolean
}

// --- Checkin ---

export interface CheckinStatus {
  total_registered: number
  total_checked_in: number
  checked_in_names: string[]
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

// --- Group & Bracket ---

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
  series_wins_team1: number
  series_wins_team2: number
  games: MatchGame[]
  scheduled_at: string | null
  played_at: string | null
}

export interface MatchGame {
  id: number
  bracket_match_id: number
  game_number: number
  status: 'pending' | 'lobby_created' | 'in_progress' | 'completed' | 'cancelled'
  steam_party_id: string | null
  party_code: string | null
  deadlock_match_id: string | null
  winner_team: 1 | 2 | null
  duration_s: number | null
  match_stats: string | null
  created_at: string
  completed_at: string | null
}

// --- Create/Update ---

export interface TournamentCreate {
  name: string
  description?: string
  team_size: number
  bracket_format: BracketFormat
  series_format?: 1 | 3 | 5
  registration_start?: string
  registration_end?: string
  checkin_start?: string | null
  invite_mode?: InviteMode
  invite_window_start?: string
  invite_window_end?: string
  lobby_settings_preset?: LobbySettingsPreset
  lobby_settings?: Record<string, unknown>
}

export interface TournamentUpdate {
  name?: string
  description?: string
  status?: TournamentStatus
  team_size?: number
  bracket_format?: BracketFormat
  series_format?: 1 | 3 | 5
  registration_start?: string
  registration_end?: string
  checkin_start?: string | null
  group_phase_start?: string
  bracket_start?: string
  invite_mode?: InviteMode
  invite_window_start?: string
  invite_window_end?: string
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

export interface MatchEventPreset {
  key: string
  label: string
  description: string
  requires_cheats: boolean
  convars: Record<string, string | number | boolean>
  reset_convars: Record<string, string | number | boolean>
}

export interface MatchEventPresetListResult {
  success: boolean
  match_id: number
  party_id: string | null
  party_code: string | null
  presets: MatchEventPreset[]
}

export interface ApplyMatchConvarsRequest {
  convars: Record<string, string | number | boolean>
}

export interface ApplyMatchConvarsResult {
  success: boolean
  match_id: number
  party_id: string
  applied_convars: Record<string, string | number | boolean>
}

export interface ApplyMatchEventPresetRequest {
  preset_key: string
  enabled?: boolean
}

export interface ApplyMatchEventPresetResult extends ApplyMatchConvarsResult {
  preset_key: string
  enabled: boolean
  label: string
  requires_cheats: boolean
}

export interface TeamMoveRequest {
  from_team_id: number
  discord_id: string
}

export interface VoiceMoveResult {
  moved: string[]
  failed: { discord_id: string; error: string }[]
}

export interface VoiceTeamMoveResult {
  team1: VoiceMoveResult
  team2: VoiceMoveResult
}

export interface VoiceChannelMember {
  user_id: number
  display_name: string
}

export interface DraftState {
  id: number
  bracket_match_id: number
  status: 'pending' | 'in_progress' | 'completed' | 'cancelled'
  current_action_index: number
  current_action_type: 'ban' | 'pick' | null
  current_team_slot: 1 | 2 | null
  bans: string[]
  picks_team1: string[]
  picks_team2: string[]
  actions: DraftAction[]
  started_by: string | null
  started_at: string | null
  completed_at: string | null
}

export interface DraftAction {
  id: number
  session_id: number
  sequence_index: number
  action_type: 'ban' | 'pick'
  team_slot: 1 | 2
  hero_name: string | null
  taken_by: string | null
  taken_at: string | null
  is_admin_forced: number
}

export interface SeriesGameResult {
  series_done: boolean
  series_winner_team: 1 | 2 | null
  wins_team1: number
  wins_team2: number
  next_game_number: number | null
}

// --- Consent ---

export interface ConsentStatus {
  has_consent: boolean
  consented_at: string | null
  consent_version: number | null
}

// --- User Profile ---

export interface UserProfile {
  discord_id: string
  bio: string | null
  display_name: string | null
  avatar_filename: string | null
  invite_auto_accept: boolean
  notify_discord_dm: boolean
  notify_browser: boolean
  updated_at: string | null
}

export interface UserProfileUpdate {
  bio?: string
  display_name?: string
  avatar_filename?: string
  invite_auto_accept?: boolean
  notify_discord_dm?: boolean
  notify_browser?: boolean
}

// --- Team Applications ---

export interface TeamApplication {
  id: number
  team_id: number
  discord_name: string
  status: ApplicationStatus
  created_at: string
}

// --- Team Invitations ---

export interface TeamInvitation {
  id: number
  tournament_id: number
  team_id: number
  team_name: string | null
  status: InvitationStatus
  created_at: string
  expires_at: string | null
}

// --- Leaderboard ---

export interface LeaderboardEntry {
  rank_position: number
  discord_name: string
  rank: string | null
  total_points: number
  tournaments_played: number
  matches_played: number
  matches_won: number
  best_placement: number | null
}

// --- Player Profile ---

export interface TournamentHistoryEntry {
  tournament_name: string
  placement: number | null
  team_name: string | null
}

export interface PlayerProfile {
  discord_name: string
  discord_avatar: string | null
  avatar_filename: string | null
  bio: string | null
  rank: string | null
  rank_score: number
  tournaments_played: number
  matches_played: number
  matches_won: number
  best_placement: number | null
  total_points: number
  tournament_history: TournamentHistoryEntry[]
}
