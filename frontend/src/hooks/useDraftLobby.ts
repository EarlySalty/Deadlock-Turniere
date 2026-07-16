/**
 * Hooks der freien Draft-Lobbys.
 *
 * Live-Sync laeuft ueber Polling im Sekundentakt, nicht ueber WebSocket/SSE:
 * der Zustand liegt vollstaendig in Postgres, der Server merkt sich nichts.
 * Dadurch ueberlebt ein laufender Draft Backend-Restarts, Reconnects und
 * Netzwechsel — bei einem Broadcast-Kanal im Prozessspeicher waere er weg.
 * Ein Zug dauert ~30s; eine Sekunde Verzoegerung faellt niemandem auf.
 */
import { useEffect, useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import {
  createDraftLobby,
  fetchDraftHeroes,
  fetchDraftLobby,
  submitDraftLobbyAction,
} from '@/api/client'
import type { CreateLobbyBody } from '@/types/tournament'
import { remainingSeconds, selectCaptainToken } from './draftLobbyState.ts'

/** Heldenliste. Live von der Deadlock-API, aendert sich hoechstens pro Patch. */
export function useDraftHeroList() {
  return useQuery({
    queryKey: ['draft', 'heroes'],
    queryFn: fetchDraftHeroes,
    staleTime: 1000 * 60 * 60,
  })
}

/** Lobby-Zustand. Sekundentakt, solange gedraftet wird; danach still. */
export function useDraftLobby(code: string | undefined) {
  return useQuery({
    queryKey: ['draft', 'lobby', code],
    queryFn: () => fetchDraftLobby(code!),
    enabled: !!code,
    refetchInterval: (query) =>
      query.state.data?.status === 'completed' ? false : 1000,
    retry: false,
  })
}

export function useCreateDraftLobby() {
  return useMutation({
    mutationFn: (body: CreateLobbyBody) => createDraftLobby(body),
  })
}

export function useDraftLobbyAction(code: string | undefined) {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: ({ token, heroName }: { token: string; heroName: string }) =>
      submitDraftLobbyAction(code!, token, heroName),
    // Die Antwort IST der neue Vollzustand — direkt in den Cache, damit die
    // eigene Wahl sofort steht und nicht bis zum naechsten Poll flackert.
    onSuccess: (state) => qc.setQueryData(['draft', 'lobby', code], state),
  })
}

/**
 * Countdown auf eine Server-Deadline.
 *
 * Der Server speichert nur `deadline_at`; der Browser zaehlt lokal runter.
 * Damit laeuft die Anzeige fluessig, obwohl nur einmal pro Sekunde ein Request
 * geht — und die Wahrheit bleibt beim Server: eine verstellte Uhr im Browser
 * dehnt die Zugzeit nicht, weil der Server beim naechsten Lesen abrechnet.
 */
export function useCountdown(deadlineAt: string | null | undefined) {
  const [nowMs, setNowMs] = useState(Date.now)

  useEffect(() => {
    if (!deadlineAt) return
    const id = window.setInterval(() => setNowMs(Date.now()), 250)
    return () => window.clearInterval(id)
  }, [deadlineAt])

  return remainingSeconds(deadlineAt, nowMs)
}

/**
 * Captain-Token dieser Lobby.
 *
 * Kommt aus dem Link (?t=…) und wird pro Draft-Code im localStorage abgelegt,
 * damit ein Neuladen den Captain nicht zum Zuschauer degradiert. Nach dem
 * Merken faellt der Token aus der URL — sonst teilt man ihn beim Kopieren der
 * Adresszeile versehentlich mit dem Gegner.
 */
export function useCaptainToken(code: string | undefined) {
  const schluessel = code ? `draft-token:${code}` : null
  const ausUrl = code ? new URLSearchParams(window.location.search).get('t') : null
  const token = selectCaptainToken(
    code,
    ausUrl,
    schluessel ? window.localStorage.getItem(schluessel) : null,
  )

  useEffect(() => {
    if (!schluessel || !ausUrl) return
    window.localStorage.setItem(schluessel, ausUrl)
    const sauber = new URL(window.location.href)
    sauber.searchParams.delete('t')
    window.history.replaceState({}, '', sauber.toString())
  }, [ausUrl, schluessel])

  return token
}
