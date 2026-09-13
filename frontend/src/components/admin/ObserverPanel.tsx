import { useMemo, useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Crosshair, Eye, Play, RefreshCw, ShieldCheck, Square, UserRoundSearch } from 'lucide-react'
import Card from '@/components/ui/Card'
import Button from '@/components/ui/Button'
import {
  createObserverSession,
  fetchObserverBot2Lease,
  fetchObserverSession,
  fetchObserverSessions,
  finishObserverSession,
  retryObserverSession,
  setObserverBot2Lease,
  setObserverMode,
} from '@/api/client'
import type { ObserverMode, ObserverSession } from '@/types/tournament'
import { useDraftHeroList } from '@/hooks/useDraftLobby'

const modeCopy: Record<ObserverMode, string> = {
  shadow: 'Shadow',
  assist: 'Assist',
  auto: 'Auto',
  manual: 'Manual',
}

const reasonCopy: Record<string, string> = {
  multikill_window: 'Multikill-Fenster',
  clutch_low_hp: 'Clutch bei wenig Leben',
  high_impact_teamfight: 'High-Impact-Teamfight',
  teamfight_cluster: 'Teamfight-Cluster',
  objective_pressure: 'Objective-Druck',
  active_fight: 'Aktiver Fight',
  best_available_pov: 'Bester verfügbarer POV',
  hold_hysteresis: 'Kamera wird bewusst gehalten',
  live_feed_stale: 'Live-Daten veraltet',
  scene_low_interest: 'Keine starke Szene',
}

function relativeTime(value: string | null) {
  if (!value) return 'nie'
  const seconds = Math.max(0, Math.round((Date.now() - new Date(value).getTime()) / 1000))
  if (seconds < 5) return 'gerade eben'
  if (seconds < 60) return `vor ${seconds}s`
  return `vor ${Math.floor(seconds / 60)}m`
}

function stateClass(state: ObserverSession['state']) {
  if (state === 'live') return 'border-green-500/30 bg-green-500/10 text-green-200'
  if (state === 'degraded' || state === 'error') return 'border-red-500/30 bg-red-500/10 text-red-200'
  if (state === 'finished') return 'border-border bg-card text-muted'
  return 'border-yellow-500/30 bg-yellow-500/10 text-yellow-100'
}

export default function ObserverPanel() {
  const qc = useQueryClient()
  const [selectedId, setSelectedId] = useState<number | null>(null)
  const [manualMatchId, setManualMatchId] = useState('')
  const [notice, setNotice] = useState<string | null>(null)

  const sessionsQuery = useQuery({
    queryKey: ['observer', 'sessions'],
    queryFn: fetchObserverSessions,
    refetchInterval: 2000,
  })
  const leaseQuery = useQuery({
    queryKey: ['observer', 'bot2', 'lease'],
    queryFn: fetchObserverBot2Lease,
    refetchInterval: 3000,
    retry: false,
  })
  const detailQuery = useQuery({
    queryKey: ['observer', 'session', selectedId],
    queryFn: () => fetchObserverSession(selectedId!),
    enabled: selectedId !== null,
    refetchInterval: 1000,
  })
  const heroesQuery = useDraftHeroList()

  const heroNames = useMemo(
    () => new Map((heroesQuery.data?.heroes ?? []).map((hero) => [hero.id, hero.name])),
    [heroesQuery.data?.heroes],
  )
  const sessions = useMemo(() => sessionsQuery.data?.sessions ?? [], [sessionsQuery.data?.sessions])
  const active = useMemo(() => sessions.filter((session) => !session.finished_at), [sessions])
  const selected = detailQuery.data?.session ?? sessions.find((session) => session.id === selectedId) ?? null
  const recentDecisions = detailQuery.data?.recent_decisions ?? []
  const recommendedDecision = selected?.recommended_account_id
    ? recentDecisions.find((decision) => decision.account_id === selected.recommended_account_id)
    : undefined
  const currentDecision = selected?.current_account_id
    ? recentDecisions.find((decision) => decision.account_id === selected.current_account_id)
    : undefined
  const heroLabel = (heroId: number | null | undefined, accountId: string | null | undefined) =>
    (heroId != null ? heroNames.get(heroId) : undefined) ?? (accountId ? `Account ${accountId}` : 'Directed')

  const invalidate = async (id?: number) => {
    await qc.invalidateQueries({ queryKey: ['observer', 'sessions'] })
    if (id) await qc.invalidateQueries({ queryKey: ['observer', 'session', id] })
  }

  const leaseMutation = useMutation({
    mutationFn: setObserverBot2Lease,
    onSuccess: async (lease) => {
      setNotice(
        lease.restart_requested
          ? 'Steam Bot 2 wechselt gerade den Account-Modus. Der Status aktualisiert sich automatisch.'
          : lease.reserved
            ? 'Steam Bot 2 ist bereits für den Observer reserviert.'
            : 'Steam Bot 2 läuft wieder normal.',
      )
      await qc.invalidateQueries({ queryKey: ['observer', 'bot2', 'lease'] })
    },
    onError: (error: Error) => setNotice(error.message),
  })

  const createMutation = useMutation({
    mutationFn: () => createObserverSession({ steam_match_id: manualMatchId.trim(), mode: 'shadow' }),
    onSuccess: async (session) => {
      setManualMatchId('')
      setSelectedId(session.id)
      setNotice('Shadow-Observer angelegt. Noch keine Kameraaktion wird ausgeführt.')
      await invalidate(session.id)
    },
    onError: (error: Error) => setNotice(error.message),
  })

  const modeMutation = useMutation({
    mutationFn: ({ id, mode }: { id: number; mode: ObserverMode }) => setObserverMode(id, mode),
    onSuccess: async (session) => {
      setNotice(`Observer-Modus: ${modeCopy[session.mode]}`)
      await invalidate(session.id)
    },
    onError: (error: Error) => setNotice(error.message),
  })

  const retryMutation = useMutation({
    mutationFn: retryObserverSession,
    onSuccess: async (session) => {
      setNotice('Live-Anbindung wird erneut versucht.')
      await invalidate(session.id)
    },
    onError: (error: Error) => setNotice(error.message),
  })

  const finishMutation = useMutation({
    mutationFn: finishObserverSession,
    onSuccess: async (session) => {
      setNotice('Observer beendet; Auto-Kamera fällt auf Deadlocks Directed Mode zurück.')
      await invalidate(session.id)
    },
    onError: (error: Error) => setNotice(error.message),
  })

  const lease = leaseQuery.data
  const autoReady = Boolean(
    selected &&
      selected.game_control_enabled &&
      selected.state === 'live' &&
      selected.last_vconsole_ok &&
      selected.last_game_connected &&
      selected.last_agent_heartbeat_at &&
      lease?.reserved &&
      !lease?.steam_connected,
  )

  return (
    <div className="space-y-5">
      <header>
        <h2 className="flex items-center gap-2 text-xl font-semibold text-foreground">
          <Crosshair size={20} className="text-primary" />
          Ingame Observer Director
        </h2>
        <p className="mt-1 max-w-3xl text-sm text-muted">
          Scrim-Drafts mit erkannter Match-ID werden automatisch im Shadow-Modus aufgenommen. Im normalen Safe Mode gibt es
          keinerlei automatisierte Spieleingaben. Auto bleibt zusätzlich zu Bot-2-Lease und Agent-Readiness serverseitig hart gesperrt.
        </p>
      </header>

      {notice && (
        <div role="status" className="rounded-lg border border-primary/30 bg-primary/10 px-4 py-3 text-sm text-foreground">
          {notice}
        </div>
      )}

      <div className="grid gap-4 xl:grid-cols-2">
        <Card className="space-y-4 p-5">
          <div className="flex items-start justify-between gap-3">
            <div>
              <h3 className="flex items-center gap-2 font-semibold text-foreground">
                <ShieldCheck size={16} className="text-primary" /> Steam Bot 2
              </h3>
              <p className="mt-1 text-xs text-muted">Account-2-Lease verhindert parallele Headless- und Game-Client-Sessions.</p>
            </div>
            <span className={`rounded-full border px-2.5 py-1 text-xs ${lease?.reserved ? 'border-green-500/30 bg-green-500/10 text-green-200' : 'border-border text-muted'}`}>
              {leaseQuery.isLoading ? 'Prüfe…' : leaseQuery.isError ? 'Nicht erreichbar' : lease?.reserved ? 'Observer reserviert' : 'Normalbetrieb'}
            </span>
          </div>
          <div className="grid grid-cols-2 gap-3 text-sm">
            <div className="rounded-lg border border-border bg-background/40 p-3">
              <div className="text-xs text-muted">Headless Steam</div>
              <div className="mt-1 font-semibold text-foreground">{lease?.steam_connected ? 'verbunden' : 'aus'}</div>
            </div>
            <div className="rounded-lg border border-border bg-background/40 p-3">
              <div className="text-xs text-muted">GC</div>
              <div className="mt-1 font-semibold text-foreground">{lease?.gc_connected ? 'verbunden' : 'aus'}</div>
            </div>
          </div>
          <div className="flex flex-wrap gap-2">
            <Button
              size="sm"
              onClick={() => leaseMutation.mutate(true)}
              disabled={leaseMutation.isPending || lease?.reserved === true || leaseQuery.isError}
            >
              <Play size={13} /> Für Observer reservieren
            </Button>
            <Button
              variant="secondary"
              size="sm"
              onClick={() => leaseMutation.mutate(false)}
              disabled={leaseMutation.isPending || !lease?.reserved}
            >
              Normalbetrieb
            </Button>
          </div>
        </Card>

        <Card className="space-y-4 p-5">
          <div>
            <h3 className="flex items-center gap-2 font-semibold text-foreground">
              <UserRoundSearch size={16} className="text-primary" /> Manueller Match-Test
            </h3>
            <p className="mt-1 text-xs text-muted">Für einen echten Scrim-Draft ist das nicht nötig; dessen Match-ID wird automatisch übernommen.</p>
          </div>
          <label className="block text-xs font-medium uppercase tracking-wider text-muted" htmlFor="observer-match-id">
            Deadlock Match-ID
          </label>
          <div className="flex gap-2">
            <input
              id="observer-match-id"
              inputMode="numeric"
              value={manualMatchId}
              onChange={(event) => setManualMatchId(event.target.value.replace(/\D/g, ''))}
              placeholder="z. B. 123456789"
              className="min-w-0 flex-1 rounded-lg border border-border bg-background px-3 py-2 text-sm text-foreground outline-none focus:border-primary"
            />
            <Button
              size="sm"
              onClick={() => createMutation.mutate()}
              disabled={!manualMatchId.trim() || createMutation.isPending}
            >
              Shadow starten
            </Button>
          </div>
        </Card>
      </div>

      <div className="grid gap-5 xl:grid-cols-[360px_1fr]">
        <Card className="p-4">
          <div className="mb-3 flex items-center justify-between">
            <h3 className="font-semibold text-foreground">Sessions</h3>
            <span className="text-xs text-muted">{active.length} aktiv</span>
          </div>
          <div className="space-y-2">
            {sessionsQuery.isLoading ? (
              <p className="py-5 text-center text-sm text-muted">Lade Observer…</p>
            ) : sessions.length === 0 ? (
              <p className="rounded-lg border border-dashed border-border px-3 py-5 text-center text-sm text-muted">Noch keine Observer-Session.</p>
            ) : (
              sessions.map((session) => (
                <button
                  key={session.id}
                  type="button"
                  onClick={() => setSelectedId(session.id)}
                  className={`w-full rounded-lg border p-3 text-left transition-colors ${selectedId === session.id ? 'border-primary/60 bg-primary/10' : 'border-border hover:bg-card-hover'}`}
                >
                  <div className="flex items-center justify-between gap-2">
                    <span className="truncate text-sm font-semibold text-foreground">
                      {session.draft_code ? `Draft ${session.draft_code}` : `Match ${session.steam_match_id ?? '?'}`}
                    </span>
                    <span className={`rounded-full border px-2 py-0.5 text-[10px] ${stateClass(session.state)}`}>{session.state}</span>
                  </div>
                  <div className="mt-2 flex items-center justify-between text-xs text-muted">
                    <span>{modeCopy[session.mode]}</span>
                    <span>POV {session.recommended_account_id ?? '—'} · {session.recommended_score?.toFixed(0) ?? '—'}</span>
                  </div>
                </button>
              ))
            )}
          </div>
        </Card>

        <Card className="min-w-0 space-y-4 p-5">
          {!selected ? (
            <div className="py-12 text-center text-muted">
              <Eye size={32} className="mx-auto mb-3 opacity-50" />
              Eine Observer-Session auswählen.
            </div>
          ) : (
            <>
              <div className="flex flex-wrap items-start justify-between gap-3">
                <div>
                  <h3 className="text-lg font-semibold text-foreground">
                    {selected.draft_code ? `${selected.draft_code} · ` : ''}Match {selected.steam_match_id ?? 'noch unbekannt'}
                  </h3>
                  <p className="mt-1 text-xs text-muted">
                    Agent {relativeTime(selected.last_agent_heartbeat_at)} · VConsole {selected.last_vconsole_ok ? 'bereit' : 'nicht bereit'} · Spiel {selected.last_game_connected ? 'verbunden' : 'nicht bestätigt'} · Live-Daten {relativeTime(selected.last_live_event_at)}
                  </p>
                </div>
                <span className={`rounded-full border px-3 py-1 text-xs ${stateClass(selected.state)}`}>{selected.state}</span>
              </div>

              <div className="grid gap-3 sm:grid-cols-3">
                <div className="rounded-lg border border-border bg-background/40 p-3">
                  <div className="text-xs text-muted">Aktueller POV</div>
                  <div className="mt-1 text-lg font-semibold text-foreground">
                    {heroLabel(currentDecision?.hero_id, selected.current_account_id)}
                  </div>
                  <div className="text-xs text-muted">
                    {selected.current_account_id ? `Account ${selected.current_account_id} · ` : ''}Score {selected.current_score?.toFixed(1) ?? '—'}
                  </div>
                </div>
                <div className="rounded-lg border border-primary/30 bg-primary/5 p-3">
                  <div className="text-xs text-muted">Empfehlung</div>
                  <div className="mt-1 text-lg font-semibold text-primary">
                    {heroLabel(recommendedDecision?.hero_id, selected.recommended_account_id)}
                  </div>
                  <div className="text-xs text-muted">
                    {selected.recommended_account_id ? `Account ${selected.recommended_account_id} · ` : ''}Score {selected.recommended_score?.toFixed(1) ?? '—'}
                  </div>
                </div>
                <div className="rounded-lg border border-border bg-background/40 p-3">
                  <div className="text-xs text-muted">Auto-Gate</div>
                  <div className={`mt-1 font-semibold ${autoReady ? 'text-green-300' : 'text-yellow-200'}`}>
                    {!selected.game_control_enabled ? 'Safe Mode' : autoReady ? 'bereit' : 'gesperrt'}
                  </div>
                  <div className="text-xs text-muted">
                    {!selected.game_control_enabled ? 'Keine automatisierten Spieleingaben' : 'Bot 2 + Agent + VConsole + Live'}
                  </div>
                </div>
              </div>

              {selected.fallback_reason && (
                <div className="rounded-lg border border-red-500/30 bg-red-500/10 px-3 py-2 text-sm text-red-100">
                  Fallback: {selected.fallback_reason}
                </div>
              )}

              <div>
                <div className="mb-2 text-xs font-semibold uppercase tracking-wider text-muted">Regie-Modus</div>
                <div className="flex flex-wrap gap-2">
                  {(['shadow', 'assist', 'auto', 'manual'] as ObserverMode[]).map((mode) => (
                    <Button
                      key={mode}
                      size="sm"
                      variant={selected.mode === mode ? 'primary' : 'secondary'}
                      disabled={modeMutation.isPending || (mode === 'auto' && !autoReady)}
                      onClick={() => modeMutation.mutate({ id: selected.id, mode })}
                    >
                      {modeCopy[mode]}
                    </Button>
                  ))}
                  {selected.state === 'degraded' && (
                    <Button size="sm" variant="outline" onClick={() => retryMutation.mutate(selected.id)} disabled={retryMutation.isPending}>
                      <RefreshCw size={12} /> Retry
                    </Button>
                  )}
                  {!selected.finished_at && (
                    <Button size="sm" variant="danger" onClick={() => finishMutation.mutate(selected.id)} disabled={finishMutation.isPending}>
                      <Square size={12} /> Beenden
                    </Button>
                  )}
                </div>
              </div>

              <div>
                <div className="mb-2 flex items-center justify-between">
                  <span className="text-xs font-semibold uppercase tracking-wider text-muted">Letzte Entscheidungen</span>
                  <span className="text-[10px] text-muted">Shadow-Daten sind der spätere Qualitätsnachweis</span>
                </div>
                <div className="max-h-[360px] overflow-auto rounded-lg border border-border">
                  <table className="w-full text-left text-xs">
                    <thead className="sticky top-0 bg-card text-muted">
                      <tr><th className="px-3 py-2">Zeit</th><th className="px-3 py-2">POV</th><th className="px-3 py-2">Score</th><th className="px-3 py-2">Grund</th><th className="px-3 py-2">Cut</th></tr>
                    </thead>
                    <tbody>
                      {recentDecisions.map((decision, index) => (
                        <tr key={`${decision.observed_at}-${index}`} className="border-t border-border/50 text-foreground/90">
                          <td className="whitespace-nowrap px-3 py-2">{new Date(decision.observed_at).toLocaleTimeString('de-DE')}</td>
                          <td className="px-3 py-2">
                            <div className="font-semibold">{heroLabel(decision.hero_id, decision.account_id)}</div>
                            {decision.account_id && <div className="font-mono text-[10px] text-muted">{decision.account_id}</div>}
                          </td>
                          <td className="px-3 py-2">{decision.score.toFixed(1)}</td>
                          <td className="px-3 py-2">{reasonCopy[decision.reason] ?? decision.reason}</td>
                          <td className="px-3 py-2">{decision.switched ? 'ja' : 'nein'}</td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </div>
              </div>
            </>
          )}
        </Card>
      </div>
    </div>
  )
}
