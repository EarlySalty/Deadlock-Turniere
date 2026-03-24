import type { UserSession, Tournament, TournamentDetail, Team } from '@/types/tournament'

const API_BASE = '/api'

export class ApiError extends Error {
  constructor(public status: number, message: string) {
    super(message)
  }
}

async function request<T>(path: string, options?: RequestInit): Promise<T> {
  const res = await fetch(`${API_BASE}${path}`, {
    credentials: 'include',
    headers: { 'Content-Type': 'application/json', ...options?.headers },
    ...options,
  })
  if (res.status === 401) {
    window.location.href = '/login'
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

// Tournaments
export const fetchTournaments = () => request<Tournament[]>('/tournaments')
export const fetchTournament = (id: number) => request<TournamentDetail>(`/tournaments/${id}`)

// Teams
export const createTeam = (tournamentId: number, name: string) =>
  request<Team>(`/tournaments/${tournamentId}/teams`, {
    method: 'POST', body: JSON.stringify({ name }),
  })
export const joinTeam = (tournamentId: number, teamId: number) =>
  request<void>(`/tournaments/${tournamentId}/teams/${teamId}/join`, { method: 'POST' })
