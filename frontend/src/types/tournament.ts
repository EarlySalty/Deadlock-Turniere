export type TournamentStatus = 'draft' | 'registration' | 'checkin' | 'group_phase' | 'bracket' | 'completed' | 'archived'
export type BracketFormat = 'single_elimination' | 'double_elimination'
export type TournamentMode = 'group_stage' | 'bracket_only'
export type TournamentGameMode = 'standard' | 'mirror' | 'all_same' | 'random_heroes' | 'single_lane'
export type MatchStatus = 'pending' | 'checkin' | 'lobby_created' | 'in_progress' | 'completed' | 'forfeit' | 'cancelled'
export type RecruitmentStatus = 'open' | 'application' | 'closed'
export type InviteMode = 'always' | 'window' | 'never'
export type Category = 'fun' | 'comp'
export type DmScope = 'fun' | 'comp' | 'all'
export type ProposalState = 'draft' | 'pending_approval' | 'approved' | 'rejected' | 'expired'
export type ProposalSource = 'bot' | 'manual'
export type VoteDecision = 'approve' | 'reject'
export type ProposalEventInput = 'submit' | 'approve' | 'reject' | 'expire'
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
  final_series_format: 1 | 3 | 5 | null
  registration_start: string | null
  registration_end: string | null
  checkin_start: string | null
  group_phase_start: string | null
  bracket_start: string | null
  bracket_format: BracketFormat
  tournament_mode: TournamentMode
  tournament_game_mode: TournamentGameMode
  auto_lobby_enabled: boolean
  invite_mode: InviteMode
  invite_window_start: string | null
  invite_window_end: string | null
  exclude_from_leaderboard: boolean
  reminder_offsets: number[]
  start_reminder_offsets: number[]
  match_objective: string
  no_show_grace_minutes: number
  is_test: boolean
  created_by: string
  created_at: string
  updated_at: string
  rules: string | null
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
  mini_groups: BracketMiniGroup[]
  signups: TournamentSignup[]
}

export interface BracketMiniGroup {
  id: number
  tournament_id: number
  round: number
  position: number
  advances_to_match_id: number | null
  advances_to_slot: 1 | 2 | null
  team_ids: number[]
  match_ids: number[]
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
  mini_groups: BracketMiniGroup[]
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
  advanced_to_bracket?: boolean
}

// --- Group & Bracket ---

export interface GroupMatch {
  id: number
  group_id: number
  team1_id: number
  team2_id: number
  winner_id: number | null
  status: MatchStatus
  steam_party_id: string | null
  party_code: string | null
  deadlock_match_id: string | null
  match_duration_s: number | null
  match_stats: string | null
  hero_assignments: Record<string, unknown> | null
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
  mini_group_id: number | null
  hero_assignments: Record<string, unknown> | null
  scheduled_at: string | null
  on_stream: boolean
  played_at: string | null
  source_match1_id?: number | null
  source_match2_id?: number | null
  loser_to_match_id?: number | null
  loser_to_slot?: number | null
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
  final_series_format?: 1 | 3 | 5
  registration_start?: string
  registration_end?: string
  checkin_start?: string | null
  invite_mode?: InviteMode
  invite_window_start?: string
  invite_window_end?: string
  lobby_settings_preset?: LobbySettingsPreset
  lobby_settings?: Record<string, unknown>
  tournament_game_mode?: TournamentGameMode
  auto_lobby_enabled?: boolean
  exclude_from_leaderboard?: boolean
  reminder_offsets?: number[]
  start_reminder_offsets?: number[]
  match_objective?: string
  no_show_grace_minutes?: number
  is_test?: boolean
  rules?: string | null
}

export interface TournamentUpdate {
  name?: string
  description?: string
  status?: TournamentStatus
  team_size?: number
  bracket_format?: BracketFormat
  force_tournament_mode?: TournamentMode
  tournament_game_mode?: TournamentGameMode
  auto_lobby_enabled?: boolean
  series_format?: 1 | 3 | 5
  final_series_format?: 1 | 3 | 5 | null
  registration_start?: string
  registration_end?: string
  checkin_start?: string | null
  group_phase_start?: string
  bracket_start?: string
  invite_mode?: InviteMode
  invite_window_start?: string
  invite_window_end?: string
  exclude_from_leaderboard?: boolean
  reminder_offsets?: number[]
  start_reminder_offsets?: number[]
  match_objective?: string
  no_show_grace_minutes?: number
  rules?: string | null
}

export interface ManualResult {
  winner_id: number
}

// --- Turnier-Automatik ---

export interface PresetConfig {
  team_size: number
  bracket_format: BracketFormat
  series_format: number
  final_series_format: number | null
  tournament_mode: TournamentMode
  tournament_game_mode: TournamentGameMode
  match_objective: string
  invite_mode: InviteMode
  reminder_offsets: string | null
  start_reminder_offsets: string | null
  rules: string | null
  description_template: string | null
}

export interface Preset extends PresetConfig {
  id: number
  name: string
  category: Category
  active: boolean
  created_by: string
  created_at: string
  updated_at: string
}

export interface NewPresetInput {
  name: string
  category: Category
  config: PresetConfig
  active?: boolean
}

export interface PresetUpdateInput {
  name: string
  category: Category
  config: PresetConfig
}

export interface Proposal {
  id: number
  preset_id: number | null
  source: ProposalSource
  proposed_start: string | null
  config_json: string
  state: ProposalState
  proposal_message_id: string | null
  channel_id: string | null
  tournament_id: number | null
  decided_at: string | null
  created_at: string
}

export interface ProposalVote {
  id: number
  proposal_id: number
  caster_discord_id: string
  decision: VoteDecision
  created_at: string
}

export interface ProposalFeedback {
  id: number
  proposal_id: number
  caster_discord_id: string
  raw_text: string
  applied_change_json: string | null
  created_at: string
}

export interface ProposalDetail {
  proposal: Proposal
  votes: ProposalVote[]
  feedback: ProposalFeedback[]
  approvals: number
}

export interface DmOptoutStatus {
  scopes: DmScope[]
}

// --- Match-Ergebnis-Meldungen & Leitstand ---

export interface MatchResultReportCreate {
  winner_team_id?: number | null
  deadlock_match_id?: string | null
  is_no_show?: boolean
  no_show_team_id?: number | null
}

export interface MatchResultReport {
  id: number
  match_type: string
  match_id: number
  tournament_id: number
  reported_by: string
  winner_team_id: number | null
  deadlock_match_id: string | null
  is_no_show: boolean
  no_show_team_id: number | null
  status: 'pending' | 'confirmed' | 'rejected'
  created_at: string
  resolved_at: string | null
  resolved_by: string | null
}

export interface ActionItem {
  report_id: number
  match_id: number
  match_round: number
  on_stream: boolean
  team1_name: string | null
  team2_name: string | null
  reported_by: string
  is_no_show: boolean
  winner_team_id: number | null
  winner_name: string | null
  no_show_team_id: number | null
  no_show_name: string | null
  deadlock_match_id: string | null
  created_at: string
  grace_minutes: number
  grace_expired: boolean
}

export interface ActionItemsResult {
  pending_reports: ActionItem[]
  no_show_grace_minutes: number
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
  notify_match_start: boolean
  notify_checkin: boolean
  notify_team_invite: boolean
  notify_tournament_news: boolean
  notify_registration_reminder: boolean
  updated_at: string | null
}

export interface UserProfileUpdate {
  bio?: string
  display_name?: string
  avatar_filename?: string
  invite_auto_accept?: boolean
  notify_discord_dm?: boolean
  notify_browser?: boolean
  notify_match_start?: boolean
  notify_checkin?: boolean
  notify_team_invite?: boolean
  notify_tournament_news?: boolean
  notify_registration_reminder?: boolean
}

export interface MatchCaster {
  discord_id: string
  display_name: string | null
  assigned_at?: string | null
  assigned_by?: string | null
}

// --- Test-Modus ---

export interface TestUser {
  discord_id: string
  display_name: string
  rank: string | null
  rank_score: number
}

export interface CreateTestTournamentRequest {
  name: string
  team_size: number
  num_teams: number
  mode: 'bracket_only' | 'group_then_bracket'
  tournament_game_mode?: TournamentGameMode
  advance_to?: 'checkin' | 'group_phase' | 'bracket'
}

export interface CreateTestTournamentResult {
  tournament_id: number
}

export interface CreateTestUsersResult {
  created: TestUser[]
}

export interface SimulateTestRoundResult {
  simulated_matches: number
}

export interface WipeTestDataResult {
  deleted_tournaments: number
  deleted_users: number
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

// --- Freie Draft-Lobbys ---

/** Held aus GET /api/draft/heroes — live von der Deadlock-API, mit Bild. */
export interface DraftHero {
  id: number
  name: string
  image_url: string
}

export type DraftPreset = 'competitive_2ban' | 'competitive_2ban_mid' | 'competitive_1ban' | 'quick_no_ban'

/** Ein Schritt der Draft-Reihenfolge dieser Lobby. */
export interface DraftSequenceStep {
  action_type: 'ban' | 'pick'
  team_slot: 1 | 2
}

/** Antwort von POST /api/draft/lobbies. Die Tokens gibt es NUR hier, einmal. */
export interface LobbyCredentials {
  code: string
  team1_token: string
  team2_token: string
}

export interface CreateLobbyBody {
  team1_name: string
  team2_name: string
  preset: DraftPreset
  round_seconds: number
  reserve_seconds: number
}

/**
 * Zustand einer freien Lobby (GET /api/draft/lobbies/{code}).
 * Enthaelt bewusst KEINE Tokens — der Zuschauer-Link darf nicht draften koennen.
 */
export interface LobbyState {
  id: number
  code: string | null
  bracket_match_id: number | null
  team1_name: string | null
  team2_name: string | null
  sequence: DraftSequenceStep[]
  round_seconds: number | null
  reserve_seconds: number | null
  team1_reserve_left: number | null
  team2_reserve_left: number | null
  /** Wann der aktuelle Zug spaetestens faellt. Der Browser zaehlt lokal runter. */
  deadline_at: string | null
  status: 'pending' | 'in_progress' | 'completed' | 'cancelled'
  current_action_index: number
  current_action_type: 'ban' | 'pick' | null
  current_team_slot: 1 | 2 | null
  bans: string[]
  picks_team1: string[]
  picks_team2: string[]
  actions: LobbyAction[]
}

export interface LobbyAction {
  id: number
  sequence_index: number
  action_type: 'ban' | 'pick'
  team_slot: 1 | 2
  hero_name: string | null
  /** true = der Timer lief ab und hat automatisch gewaehlt. */
  is_auto: boolean
}

// --- Auto-Observer / Scrim-Regie ---

export type ObserverMode = 'shadow' | 'assist' | 'auto' | 'manual'
export type ObserverState = 'waiting' | 'pairing' | 'live' | 'degraded' | 'finished' | 'error'

export interface ObserverSession {
  id: number
  session_key: string
  scrim_match_id: number | null
  draft_code: string | null
  steam_match_id: string | null
  lobby_party_id: string | null
  bot_account_id: number
  mode: ObserverMode
  state: ObserverState
  enabled: boolean
  current_account_id: string | null
  current_score: number | null
  recommended_account_id: string | null
  recommended_score: number | null
  fallback_reason: string | null
  last_live_event_at: string | null
  last_agent_heartbeat_at: string | null
  last_agent_version: string | null
  last_vconsole_ok: boolean | null
  last_game_connected: boolean | null
  /** False = Safe Mode: keinerlei automatisierte Spieleingaben. */
  game_control_enabled: boolean
  created_at: string
  updated_at: string
  finished_at: string | null
}

export interface ObserverDecision {
  observed_at: string
  account_id: string | null
  hero_id: number | null
  score: number
  current_score: number | null
  switched: boolean
  reason: string
  factors: Record<string, number>
  frame_age_ms: number | null
}

export interface ObserverSessionDetail {
  session: ObserverSession
  recent_decisions: ObserverDecision[]
}

export interface ObserverBot2Lease {
  bot_account_id: number
  reserved: boolean
  steam_connected: boolean
  gc_connected: boolean
  restart_requested: boolean
}
