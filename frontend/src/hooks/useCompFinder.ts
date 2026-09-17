import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import type { CompPreference, CompRoom } from '@/types/comp'
import { compTokenKey, newestRoom } from './compState'

const API = `${import.meta.env.BASE_URL}api/comp/lobbies`
const roomKey = (code: string) => ['comp', code] as const

export class CompApiError extends Error {
  status: number
  constructor(status: number, message: string) {
    super(message)
    this.status = status
  }
}

function readToken(code: string): string | null {
  try { return window.sessionStorage.getItem(compTokenKey(code)) } catch { return null }
}

function makeToken(): string {
  if (!globalThis.crypto?.randomUUID) throw new Error('Bitte den Comp-Finder über HTTPS öffnen.')
  return crypto.randomUUID()
}

function storeToken(code: string, token: string) {
  try { window.sessionStorage.setItem(compTokenKey(code), token) } catch {
    throw new Error('Der Browser blockiert den Sitzungsspeicher. Bitte für diese Website erlauben, damit dein Spielerplatz gespeichert werden kann.')
  }
}

function ensureToken(code: string): string {
  const token = readToken(code) ?? makeToken()
  storeToken(code, token)
  return token
}

async function request<T>(path: string, options: { method?: string; body?: unknown; token?: string | null; signal?: AbortSignal } = {}): Promise<T> {
  const response = await fetch(`${API}${path}`, {
    method: options.method ?? 'GET',
    cache: 'no-store',
    signal: options.signal,
    headers: {
      ...(options.body !== undefined ? { 'Content-Type': 'application/json' } : {}),
      ...(options.token ? { 'X-Comp-Token': options.token } : {}),
    },
    ...(options.body !== undefined ? { body: JSON.stringify(options.body) } : {}),
  })
  if (!response.ok) {
    const error = await response.json().catch(() => ({}))
    throw new CompApiError(response.status, error.detail || 'Der Comp-Finder antwortet gerade nicht. Bitte erneut versuchen.')
  }
  return response.json() as Promise<T>
}

export function useCompRoom(code: string) {
  const qc = useQueryClient()
  return useQuery({
    queryKey: roomKey(code),
    queryFn: async ({ signal }) => {
      const incoming = await request<CompRoom>(`/${encodeURIComponent(code)}`, { token: readToken(code), signal })
      return newestRoom(qc.getQueryData<CompRoom>(roomKey(code)), incoming)
    },
    refetchInterval: query => {
      const error = query.state.error
      if (error instanceof CompApiError && error.status === 404) return false
      return error ? 15_000 : 2_000
    },
    retry: false,
  })
}

export function useCreateComp() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: async (name: string) => {
      const token = makeToken()
      storeToken('creating', token) // fail before the API call when storage is unavailable
      const room = await request<CompRoom>('', { method: 'POST', token, body: { name } })
      storeToken(room.code, token)
      window.sessionStorage.removeItem(compTokenKey('creating'))
      return room
    },
    onSuccess: room => qc.setQueryData(roomKey(room.code), room),
  })
}

function useRoomMutation<T>(code: string, path: string, body: (input: T) => unknown) {
  const qc = useQueryClient()
  return useMutation({
    onMutate: () => qc.cancelQueries({ queryKey: roomKey(code) }),
    mutationFn: (input: T) => request<CompRoom>(`/${encodeURIComponent(code)}/${path}`, {
      method: 'POST', token: ensureToken(code), body: body(input),
    }),
    onSuccess: room => qc.setQueryData<CompRoom>(roomKey(code), old => newestRoom(old, room)),
    onError: () => qc.invalidateQueries({ queryKey: roomKey(code) }),
  })
}

export function useJoinComp(code: string) {
  return useRoomMutation(code, 'join', (name: string) => ({ name }))
}

export function useSaveComp(code: string) {
  return useRoomMutation(code, 'preferences', (body: { revision: number; preferences: CompPreference[] }) => body)
}

export function useRemoveComp(code: string) {
  return useRoomMutation(code, 'remove', (member_id: string) => ({ member_id }))
}

export function useLeaveComp(code: string) {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: () => request<{ left: true }>(`/${encodeURIComponent(code)}/leave`, { method: 'POST', token: readToken(code) }),
    onSuccess: () => {
      window.sessionStorage.removeItem(compTokenKey(code))
      qc.removeQueries({ queryKey: roomKey(code) })
    },
  })
}
