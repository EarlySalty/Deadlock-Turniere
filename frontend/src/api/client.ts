import type {
  UserSession,
  Tournament,
  TournamentDetail,
  TournamentDetailPublic,
  MyTournamentStatus,
  Team,
  TournamentSignup,
  TournamentCreate,
  TournamentUpdate,
  ManualResult,
  LobbyCreateResult,
  MatchStartResult,
  MatchFetchResult,
  MatchEventPresetListResult,
  ApplyMatchConvarsRequest,
  ApplyMatchConvarsResult,
  ApplyMatchEventPresetRequest,
  ApplyMatchEventPresetResult,
  CheckinStatus,
  FinalizeCheckinResult,
  TeamMoveRequest,
  ConsentStatus,
  UserProfile,
  UserProfileUpdate,
  TeamApplication,
  TeamInvitation,
  LeaderboardEntry,
  PlayerProfile,
} from '@/types/tournament'

const API_BASE = '/turnier/api'

export class ApiError extends Error {
  status: number
  constructor(status: number, message: string) {
    super(message)
    this.status = status
  }
}

async function request<T>(path: string, options?: RequestInit): Promise<T> {
  const res = await fetch(`${API_BASE}${path}`, {
    credentials: 'include',
    headers: { 'Content-Type': 'application/json', ...options?.headers },
    ...options,
  })
  if (res.status === 401) {
    throw new ApiError(401, 'Nicht authentifiziert')
  }
  if (!res.ok) {
    const body = await res.json().catch(() => ({}))
    throw new ApiError(res.status, body.detail || 'Fehler')
  }
  if (res.status === 204) return undefined as T
  return res.json()
}

// Auth
export const fetchMe = () => request<UserSession>('/me')

// Tournaments (public)
export const fetchTournaments = () => request<Tournament[]>('/tournaments')
export const fetchTournament = (id: number) => request<TournamentDetailPublic>(`/tournaments/${id}`)
export const fetchMyTournamentStatus = (id: number) => request<MyTournamentStatus>(`/tournaments/${id}/me`)

// Tournaments (admin)
export const fetchAdminTournaments = () => request<Tournament[]>('/admin/tournaments')
export const fetchAdminTournament = (id: number) => request<TournamentDetail>(`/admin/tournaments/${id}`)

// Teams
export const createTeam = (tournamentId: number, name: string) =>
  request<Team>(`/tournaments/${tournamentId}/teams`, {
    method: 'POST', body: JSON.stringify({ name }),
  })
export const joinTeam = (tournamentId: number, teamId: number) =>
  request<void>(`/tournaments/${tournamentId}/teams/${teamId}/join`, { method: 'POST' })

// Admin
export const createTournament = (data: TournamentCreate) =>
  request<Tournament>('/admin/tournaments', { method: 'POST', body: JSON.stringify(data) })

export const updateTournament = (id: number, data: TournamentUpdate) =>
  request<Tournament>(`/admin/tournaments/${id}`, { method: 'PUT', body: JSON.stringify(data) })

export const deleteTournament = (id: number) =>
  request<void>(`/admin/tournaments/${id}`, { method: 'DELETE' })

export const advanceTournament = (id: number) =>
  request<Tournament>(`/admin/tournaments/${id}/advance`, { method: 'POST' })

export const openCheckin = (id: number) =>
  request<Tournament>(`/admin/tournaments/${id}/open-checkin`, { method: 'POST' })

export const finalizeCheckin = (
  id: number,
  confirm: boolean,
  allowedTeamIds: number[] = [],
  snapshotToken?: string,
) => request<FinalizeCheckinResult>(
  `/admin/tournaments/${id}/finalize-checkin${confirm ? '?confirm=true' : ''}`,
  {
    method: 'POST',
    body: JSON.stringify({ allowed_team_ids: allowedTeamIds, snapshot_token: snapshotToken }),
  }
)

export const assignRandomTeams = (id: number) =>
  request<{ teams_created: number }>(`/admin/tournaments/${id}/assign-random`, { method: 'POST' })

export const submitMatchResult = (
  tournamentId: number,
  matchId: number,
  data: ManualResult,
  options?: { force?: boolean },
) => {
  const query = options?.force ? '?force=true' : ''
  return request<void>(`/admin/tournaments/${tournamentId}/matches/${matchId}/result${query}`, {
    method: 'POST', body: JSON.stringify(data),
  })
}

export const createLobby = (tournamentId: number, matchId: number) =>
  request<LobbyCreateResult>(`/admin/tournaments/${tournamentId}/matches/${matchId}/create-lobby`, {
    method: 'POST',
  })

export const startMatch = (tournamentId: number, matchId: number) =>
  request<MatchStartResult>(`/admin/tournaments/${tournamentId}/matches/${matchId}/start`, {
    method: 'POST',
  })

export const fetchMatchResult = (tournamentId: number, matchId: number) =>
  request<MatchFetchResult>(`/admin/tournaments/${tournamentId}/matches/${matchId}/fetch-result`, {
    method: 'POST',
  })

export const leaveLobby = (tournamentId: number, matchId: number) =>
  request<{ success: boolean }>(`/admin/tournaments/${tournamentId}/matches/${matchId}/leave-lobby`, {
    method: 'POST',
  })

export const fetchMatchEventPresets = (tournamentId: number, matchId: number) =>
  request<MatchEventPresetListResult>(
    `/admin/tournaments/${tournamentId}/matches/${matchId}/event-presets`
  )

export const applyMatchConvars = (
  tournamentId: number,
  matchId: number,
  data: ApplyMatchConvarsRequest,
) =>
  request<ApplyMatchConvarsResult>(
    `/admin/tournaments/${tournamentId}/matches/${matchId}/apply-convars`,
    {
      method: 'POST',
      body: JSON.stringify(data),
    }
  )

export const applyMatchEventPreset = (
  tournamentId: number,
  matchId: number,
  data: ApplyMatchEventPresetRequest,
) =>
  request<ApplyMatchEventPresetResult>(
    `/admin/tournaments/${tournamentId}/matches/${matchId}/apply-event-preset`,
    {
      method: 'POST',
      body: JSON.stringify(data),
    }
  )

export const createAdminTeam = (tournamentId: number, name: string) =>
  request<Team>(`/admin/tournaments/${tournamentId}/teams`, {
    method: 'POST',
    body: JSON.stringify({ name }),
  })

export const renameAdminTeam = (tournamentId: number, teamId: number, name: string) =>
  request<Team>(`/admin/tournaments/${tournamentId}/teams/${teamId}`, {
    method: 'PUT',
    body: JSON.stringify({ name }),
  })

export const deleteAdminTeam = (tournamentId: number, teamId: number) =>
  request<{ status: string; team_id: number }>(`/admin/tournaments/${tournamentId}/teams/${teamId}`, {
    method: 'DELETE',
  })

export const changeAdminCaptain = (tournamentId: number, teamId: number, discordId: string) =>
  request<Team>(`/admin/tournaments/${tournamentId}/teams/${teamId}/captain`, {
    method: 'PUT',
    body: JSON.stringify({ discord_id: discordId }),
  })

export const removeAdminTeamMember = (tournamentId: number, teamId: number, discordId: string) =>
  request<Team>(`/admin/tournaments/${tournamentId}/teams/${teamId}/members/${discordId}`, {
    method: 'DELETE',
  })

export const moveAdminTeamMember = (tournamentId: number, targetTeamId: number, data: TeamMoveRequest) =>
  request<Team>(`/admin/tournaments/${tournamentId}/teams/${targetTeamId}/members/move`, {
    method: 'POST',
    body: JSON.stringify(data),
  })

export const assignSignupToAdminTeam = (tournamentId: number, teamId: number, signupId: number) =>
  request<Team>(`/admin/tournaments/${tournamentId}/teams/${teamId}/signups/assign`, {
    method: 'POST',
    body: JSON.stringify({ signup_id: signupId }),
  })

export const addTeamMember = (
  tournamentId: number,
  teamId: number,
  discordId: string,
  discordName: string,
) =>
  request<Team>(`/admin/tournaments/${tournamentId}/teams/${teamId}/add-member`, {
    method: 'POST',
    body: JSON.stringify({ discord_id: discordId, discord_name: discordName }),
  })

export const deleteAdminSignup = (tournamentId: number, signupId: number) =>
  request<TournamentSignup>(`/admin/tournaments/${tournamentId}/signups/${signupId}`, {
    method: 'DELETE',
  })

// Groups & Bracket Generation
export const generateGroups = (tournamentId: number, numGroups: number = 4) =>
  request<{ groups_created: number; matches_created: number }>(
    `/admin/tournaments/${tournamentId}/groups/generate`,
    { method: 'POST', body: JSON.stringify({ num_groups: numGroups }) }
  )

export const generateBracket = (tournamentId: number) =>
  request<{ bracket_matches_created: number }>(
    `/admin/tournaments/${tournamentId}/bracket/generate`,
    { method: 'POST' }
  )

// Solo Signup
export const signupSolo = (tournamentId: number) =>
  request<void>(`/tournaments/${tournamentId}/signup`, { method: 'POST' })

export const checkinPlayer = (tournamentId: number) =>
  request<{ checked_in: boolean; already_checked_in: boolean }>(`/tournaments/${tournamentId}/checkin`, { method: 'POST' })

export const getCheckinStatus = (tournamentId: number) =>
  request<CheckinStatus>(`/tournaments/${tournamentId}/checkin-status`)

export const withdrawSolo = (tournamentId: number) =>
  request<void>(`/tournaments/${tournamentId}/signup`, { method: 'DELETE' })

export const kickMember = (tournamentId: number, teamId: number, discordId: string) =>
  request<void>(`/tournaments/${tournamentId}/teams/${teamId}/members/${discordId}`, { method: 'DELETE' })

export const inviteBySignup = (tournamentId: number, teamId: number, signupId: number) =>
  request<{ status: string }>(`/tournaments/${tournamentId}/teams/${teamId}/invite-by-signup/${signupId}`, { method: 'POST' })

export const leaveTeam = (tournamentId: number, teamId: number) =>
  request<void>(`/tournaments/${tournamentId}/teams/${teamId}/leave`, { method: 'DELETE' })

// Recruiting Status
export const setRecruitingStatus = (tournamentId: number, teamId: number, recruitmentStatus: string) =>
  request<void>(`/tournaments/${tournamentId}/teams/${teamId}/recruiting`, {
    method: 'PATCH',
    body: JSON.stringify({ recruitment_status: recruitmentStatus }),
  })

// Invitations
export const fetchMyInvitations = (tournamentId: number) =>
  request<TeamInvitation[]>(`/tournaments/${tournamentId}/my-invitations`)

export const acceptInvitation = (tournamentId: number, inviteId: number) =>
  request<void>(`/tournaments/${tournamentId}/invitations/${inviteId}/accept`, { method: 'POST' })

export const rejectInvitation = (tournamentId: number, inviteId: number) =>
  request<void>(`/tournaments/${tournamentId}/invitations/${inviteId}/reject`, { method: 'POST' })

// Applications
export const applyToTeam = (tournamentId: number, teamId: number) =>
  request<void>(`/tournaments/${tournamentId}/teams/${teamId}/apply`, { method: 'POST' })

export const fetchTeamApplications = (tournamentId: number, teamId: number) =>
  request<TeamApplication[]>(`/tournaments/${tournamentId}/teams/${teamId}/applications`)

export const acceptApplication = (tournamentId: number, teamId: number, appId: number) =>
  request<void>(`/tournaments/${tournamentId}/teams/${teamId}/applications/${appId}/accept`, { method: 'POST' })

export const rejectApplication = (tournamentId: number, teamId: number, appId: number) =>
  request<void>(`/tournaments/${tournamentId}/teams/${teamId}/applications/${appId}/reject`, { method: 'POST' })

// Consent
export const fetchConsent = () => request<ConsentStatus>('/consent')
export const setConsent = (version: number = 1) =>
  request<ConsentStatus>('/consent', { method: 'POST', body: JSON.stringify({ consent_version: version }) })

// Profile
export const fetchMyProfile = () => request<UserProfile>('/profile')
export const updateMyProfile = (data: UserProfileUpdate) =>
  request<UserProfile>('/profile', { method: 'PUT', body: JSON.stringify(data) })
export const uploadProfileAvatar = async (file: File): Promise<UserProfile> => {
  const formData = new FormData()
  formData.append('avatar', file)
  const res = await fetch(`${API_BASE}/profile/avatar`, {
    method: 'POST',
    credentials: 'include',
    body: formData,
  })
  if (res.status === 401) throw new ApiError(401, 'Nicht authentifiziert')
  if (!res.ok) {
    const body = await res.json().catch(() => ({}))
    throw new ApiError(res.status, body.detail || 'Upload fehlgeschlagen')
  }
  return res.json()
}

// Leaderboard & Player Profiles
export const fetchLeaderboard = () => request<LeaderboardEntry[]>('/leaderboard')
export const fetchPlayerProfile = (discordName: string) =>
  request<PlayerProfile>(`/players/${encodeURIComponent(discordName)}`)
