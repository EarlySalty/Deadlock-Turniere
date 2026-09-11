import { useEffect, useMemo, useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import type {
  DraftHero,
  DraftRaumAnlegen,
  DraftRaumCode,
  DraftRaumZustand,
  DraftClaimAntwort,
  DraftTeamSeite,
} from '@/types/draft'
import {
  countdownText,
  leseClaimToken,
  phasenKopf,
  remainingSeconds,
  schreibeClaimToken,
} from './draftLobbyState.ts'
import {
  DraftMockFehler,
  mockAktion,
  mockAnlegen,
  mockClaim,
  mockHelden,
  mockLeave,
  mockLobbyRetry,
  mockReady,
  mockRematch,
  mockZustand,
} from '@/mocks/draftFixtures'

const MOCK_AKTIV = import.meta.env.DEV && import.meta.env.VITE_DRAFT_MOCK === '1'
const API_BASE = '/turnier/api'
const VIEWER_SPEICHER = 'draft-viewer'

export class DraftApiFehler extends Error {
  status: number
  constructor(status: number, message: string) {
    super(message)
    this.status = status
  }
}

async function draftFetch<T>(
  pfad: string,
  viewerId: string,
  options?: RequestInit & { claim?: string | null },
): Promise<T> {
  if (MOCK_AKTIV) {
    return mockAntwort<T>(pfad, viewerId, options)
  }
  const { claim, ...rest } = options ?? {}
  const res = await fetch(`${API_BASE}${pfad}`, {
    ...rest,
    headers: {
      'Content-Type': 'application/json',
      'X-Draft-Viewer': viewerId,
      ...(claim ? { 'X-Draft-Token': claim } : {}),
      ...rest.headers,
    },
  })
  if (!res.ok) {
    const body = await res.json().catch(() => ({}))
    throw new DraftApiFehler(res.status, body.detail || 'Der Draft antwortet gerade nicht')
  }
  return res.json() as Promise<T>
}

async function mockAntwort<T>(
  pfad: string,
  viewerId: string,
  options?: RequestInit & { claim?: string | null },
): Promise<T> {
  const body = options?.body ? (JSON.parse(options.body as string) as Record<string, unknown>) : {}
  const claim = options?.claim ?? null
  const method = options?.method ?? 'GET'
  const teile = pfad.replace(/^\/draft\/lobbies\//, '').split('/')
  const code = decodeURIComponent(teile[0] ?? '')
  const rest = teile.slice(1)
  const antwort = (wert: unknown) => wert as T
  const warte = (ms: number) => new Promise((r) => setTimeout(r, ms))

  if (pfad === '/draft/heroes') return antwort({ heroes: mockHelden() })
  if (pfad === '/draft/lobbies' && method === 'POST') return antwort(mockAnlegen(body as DraftRaumAnlegen))
  if (rest.length === 0 && method === 'GET') {
    return antwort(mockZustand(code, viewerId, claim))
  }
  if (rest[0] === 'claim' && method === 'POST') {
    return antwort(mockClaim(code, body.team as DraftTeamSeite, viewerId))
  }
  if (rest[0] === 'ready' && method === 'POST') {
    mockReady(code, claim, viewerId)
    await warte(120)
    return antwort(mockZustand(code, viewerId, claim))
  }
  if (rest[0] === 'leave' && method === 'POST') {
    mockLeave(code, claim, viewerId)
    await warte(120)
    return antwort(mockZustand(code, viewerId, claim))
  }
  if (rest[0] === 'action' && method === 'POST') {
    mockAktion(code, body.hero_name as string, claim, viewerId)
    return antwort(mockZustand(code, viewerId, claim))
  }
  if (rest[0] === 'rematch' && method === 'POST') {
    await warte(150)
    return antwort(mockRematch(code))
  }
  if (rest[0] === 'lobby' && rest[1] === 'retry' && method === 'POST') {
    mockLobbyRetry(code, viewerId)
    return antwort(mockZustand(code, viewerId, claim))
  }
  throw new DraftMockFehler(404, 'Diese Draft-Route gibt es im Mock nicht')
}

function leseOderErzeugeViewerId(): string {
  const vorhanden = window.sessionStorage.getItem(VIEWER_SPEICHER)
  if (vorhanden) return vorhanden
  const neu =
    typeof crypto !== 'undefined' && 'randomUUID' in crypto
      ? crypto.randomUUID()
      : `viewer-${Math.random().toString(36).slice(2)}${Date.now()}`
  window.sessionStorage.setItem(VIEWER_SPEICHER, neu)
  return neu
}

export function useViewerId(): string {
  return leseOderErzeugeViewerId()
}

export function useDraftHeroList() {
  const viewerId = useViewerId()
  return useQuery({
    queryKey: ['draft', 'helden'],
    queryFn: () => draftFetch<{ heroes: DraftHero[] }>('/draft/heroes', viewerId),
    staleTime: 1000 * 60 * 60,
  })
}

export function useScrimLobby(code: string | undefined) {
  const viewerId = useViewerId()
  return useQuery({
    queryKey: ['draft', 'scrim', code, viewerId],
    queryFn: () =>
      draftFetch<DraftRaumZustand>(
        `/draft/lobbies/${encodeURIComponent(code!)}`,
        viewerId,
        { claim: leseClaimToken(code!) },
      ),
    enabled: !!code,
    refetchIntervalInBackground: true,
    refetchInterval: (query) => {
      const s = query.state.data
      if (
        s &&
        s.phase === 'abgeschlossen' &&
        ['bereit', 'fehler', 'gestartet', 'beendet'].includes(s.lobby.status)
      ) {
        return false
      }
      return 1000
    },
    retry: false,
  })
}

export function useDraftRaumAnlegen() {
  const viewerId = useViewerId()
  return useMutation({
    mutationFn: (body: DraftRaumAnlegen) =>
      draftFetch<DraftRaumCode>('/draft/lobbies', viewerId, {
        method: 'POST',
        body: JSON.stringify(body),
      }),
  })
}

function useRaumPost(code: string | undefined, abschnitt: string) {
  const viewerId = useViewerId()
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (koerper: Record<string, unknown> = {}) =>
      draftFetch<DraftRaumZustand>(
        `/draft/lobbies/${encodeURIComponent(code!)}/${abschnitt}`,
        viewerId,
        {
          method: 'POST',
          body: JSON.stringify(koerper),
          claim: code ? leseClaimToken(code) : null,
        },
      ),
    onSuccess: (data) => {
      if (code) qc.setQueryData(['draft', 'scrim', code, viewerId], data)
    },
  })
}

export function useClaimCaptain(code: string | undefined) {
  const viewerId = useViewerId()
  const qc = useQueryClient()
  return useMutation({
    mutationFn: async (team: DraftTeamSeite) => {
      const antwort = await draftFetch<DraftClaimAntwort>(
        `/draft/lobbies/${encodeURIComponent(code!)}/claim`,
        viewerId,
        { method: 'POST', body: JSON.stringify({ team }) },
      )
      schreibeClaimToken(code!, antwort.token)
      return antwort
    },
    onSuccess: () => qc.invalidateQueries({ queryKey: ['draft', 'scrim', code] }),
  })
}

export function useReadyTeam(code: string | undefined) {
  return useRaumPost(code, 'ready')
}

export function useLeaveCaptain(code: string | undefined) {
  return useRaumPost(code, 'leave')
}

export function useDraftAktion(code: string | undefined) {
  const viewerId = useViewerId()
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (heldName: string) =>
      draftFetch<DraftRaumZustand>(
        `/draft/lobbies/${encodeURIComponent(code!)}/action`,
        viewerId,
        {
          method: 'POST',
          body: JSON.stringify({ hero_name: heldName }),
          claim: code ? leseClaimToken(code) : null,
        },
      ),
    onSuccess: (data) => {
      if (code) qc.setQueryData(['draft', 'scrim', code, viewerId], data)
    },
  })
}

export function useLobbyRetry(code: string | undefined) {
  return useRaumPost(code, 'lobby/retry')
}

export function useRematch(code: string | undefined) {
  const viewerId = useViewerId()
  return useMutation({
    mutationFn: () =>
      draftFetch<DraftRaumCode>(`/draft/lobbies/${encodeURIComponent(code!)}/rematch`, viewerId, {
        method: 'POST',
      }),
  })
}

export function useCountdown(deadlineAt: string | null | undefined) {
  const [nowMs, setNowMs] = useState(Date.now)

  useEffect(() => {
    if (!deadlineAt) return
    const id = window.setInterval(() => setNowMs(Date.now()), 250)
    return () => window.clearInterval(id)
  }, [deadlineAt])

  return remainingSeconds(deadlineAt, nowMs)
}

export function useCountdownText(restSekunden: number | null) {
  return useMemo(() => countdownText(restSekunden), [restSekunden])
}

export function usePhasenKopf(sequenz: DraftRaumZustand['sequence'], aktuellerIndex: number) {
  return useMemo(() => phasenKopf(sequenz, aktuellerIndex), [sequenz, aktuellerIndex])
}
