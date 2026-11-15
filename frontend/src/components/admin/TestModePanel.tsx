import { useEffect, useRef, useState } from 'react'
import Card from '@/components/ui/Card'
import Button from '@/components/ui/Button'
import {
  useTestUsers,
  useCreateTestUsers,
  useWipeTestUsers,
  useCreateTestTournament,
  useSimulateTestRound,
  useWipeTestData,
  useAdminTournaments,
  useTestTournamentOptions,
} from '@/hooks/useTournament'
import type {
  CreateTestTournamentRequest,
  TestAdvance,
  TestTournamentOptions,
  TournamentGameMode,
  TournamentMode,
} from '@/types/tournament'
import { GAME_MODE_OPTIONS, MODE_OPTIONS } from '@/components/admin/settings/options'
import {
  AlertTriangle,
  CheckCircle2,
  FlaskConical,
  Play,
  Plus,
  Trash2,
  Users,
} from 'lucide-react'
import { pluralize } from '@/lib/countLabel'

type StatusMsg = { kind: 'success' | 'error'; text: string }

function advanceLabel(target: TestAdvance, mode: TournamentMode): string {
  if (target === 'checkin') return 'Check-in'
  if (target === 'bracket') return 'Bracket'
  return mode === 'league' ? 'Spielplan' : 'Gruppenphase'
}

function allowedTargets(
  options: TestTournamentOptions | undefined,
  mode: TournamentMode,
  soloSignups: boolean,
): TestAdvance[] {
  return (
    options?.advance_rules.find((rule) => rule.mode === mode && rule.solo_signups === soloSignups)
      ?.advance_to ?? ['checkin']
  )
}

function keepAllowed(targets: TestAdvance[], current: TestAdvance | undefined): TestAdvance {
  return current && targets.includes(current) ? current : 'checkin'
}

export default function TestModePanel() {
  const { data: testUsers = [], isLoading: loadingUsers } = useTestUsers()
  const { data: allTournaments = [] } = useAdminTournaments()
  const { data: options } = useTestTournamentOptions()
  const createUsers = useCreateTestUsers()
  const wipeUsers = useWipeTestUsers()
  const createTournament = useCreateTestTournament()
  const simulateRound = useSimulateTestRound()
  const wipeAll = useWipeTestData()

  const [userCount, setUserCount] = useState(8)
  const [tournamentForm, setTournamentForm] = useState<CreateTestTournamentRequest>({
    name: 'Test-Turnier',
    team_size: 6,
    num_teams: 4,
    mode: 'bracket_only',
    tournament_game_mode: 'standard',
    advance_to: 'bracket',
  })
  const [status, setStatus] = useState<StatusMsg | null>(null)
  const statusTimer = useRef<number | null>(null)
  useEffect(
    () => () => {
      if (statusTimer.current !== null) window.clearTimeout(statusTimer.current)
    },
    [],
  )

  const testTournaments = allTournaments.filter((t) => t.is_test)
  const isBusy =
    createUsers.isPending ||
    wipeUsers.isPending ||
    createTournament.isPending ||
    simulateRound.isPending ||
    wipeAll.isPending

  const showStatus = (msg: StatusMsg) => {
    setStatus(msg)
    if (statusTimer.current !== null) window.clearTimeout(statusTimer.current)
    statusTimer.current = window.setTimeout(() => setStatus(null), 6000)
  }

  const handleCreateUsers = async () => {
    if (userCount < 1) return
    try {
      const result = await createUsers.mutateAsync(userCount)
      showStatus({
        kind: 'success',
        text: `${result.created.length} Test-User erstellt`,
      })
    } catch (err) {
      showStatus({
        kind: 'error',
        text: `Fehler: ${(err as Error).message}`,
      })
    }
  }

  const handleWipeUsers = async () => {
    if (!window.confirm('Alle Test-User löschen?')) return
    try {
      const result = await wipeUsers.mutateAsync()
      showStatus({ kind: 'success', text: `${result.deleted} Test-User entfernt` })
    } catch (err) {
      showStatus({ kind: 'error', text: `Fehler: ${(err as Error).message}` })
    }
  }

  const handleCreateTournament = async () => {
    try {
      const result = await createTournament.mutateAsync(tournamentForm)
      showStatus({
        kind: 'success',
        text: `Test-Turnier #${result.tournament_id} angelegt. Wechsle in den Live-Tab, um es zu öffnen`,
      })
    } catch (err) {
      showStatus({ kind: 'error', text: `Fehler: ${(err as Error).message}` })
    }
  }

  const handleSimulateRound = async (tournamentId: number) => {
    try {
      const result = await simulateRound.mutateAsync(tournamentId)
      if (result.simulated_matches === 0) {
        showStatus({ kind: 'error', text: 'Keine offenen Matches mit zwei Teams gefunden, es wurde nichts gewürfelt.' })
        return
      }
      showStatus({
        kind: 'success',
        text: `${pluralize(result.simulated_matches, 'Match', 'Matches')} gewürfelt. Lade Detail erneut, um Folge-Runden zu sehen`,
      })
    } catch (err) {
      showStatus({ kind: 'error', text: `Fehler: ${(err as Error).message}` })
    }
  }

  const handleWipeAll = async () => {
    if (
      !window.confirm(
        'WIRKLICH ALLES wipen? Alle Test-User UND alle Test-Turniere werden gelöscht.',
      )
    )
      return
    try {
      const result = await wipeAll.mutateAsync()
      showStatus({
        kind: 'success',
        text: `${pluralize(result.deleted_tournaments, 'Test-Turnier', 'Test-Turniere')} und ${result.deleted_users} Test-User entfernt`,
      })
    } catch (err) {
      showStatus({ kind: 'error', text: `Fehler: ${(err as Error).message}` })
    }
  }

  return (
    <div className="space-y-6">
      <header className="flex flex-col gap-2">
        <div className="flex items-center gap-2">
          <FlaskConical size={20} className="text-amber-400" />
          <h2 className="text-lg font-semibold text-foreground">Test-Modus</h2>
        </div>
        <p className="text-sm text-muted">
          Erstelle Test-User, Test-Turniere und simuliere ganze Runden, ohne echte Discord-
          oder Steam-Lobbies anzulegen. Test-Turniere haben das Flag <code>is_test=1</code>{' '}
          und sind von Discord-Notifier und Auto-Lobby ausgeschlossen.
        </p>
      </header>

      {status && (
        <div
          className={`flex items-center gap-2 rounded-lg p-3 text-sm ${
            status.kind === 'error'
              ? 'border border-red-500/20 bg-red-500/10 text-red-400'
              : 'border border-green-500/20 bg-green-500/10 text-green-400'
          }`}
        >
          {status.kind === 'error' ? <AlertTriangle size={16} /> : <CheckCircle2 size={16} />}
          <span>{status.text}</span>
        </div>
      )}

      <Card>
        <div className="space-y-4">
          <div className="flex items-center gap-2">
            <Users size={16} className="text-primary" />
            <h3 className="font-semibold text-foreground">
              Test-User ({loadingUsers ? '...' : testUsers.length})
            </h3>
          </div>
          <div className="flex flex-wrap items-end gap-2">
            <div className="flex flex-col gap-1">
              <label className="text-xs text-muted" htmlFor="test-user-count">
                Anzahl
              </label>
              <input
                id="test-user-count"
                type="number"
                min={1}
                max={options?.user_count_max}
                value={userCount}
                onChange={(e) => setUserCount(Math.max(1, Number(e.target.value) || 1))}
                className="w-24 rounded-lg border border-border bg-background px-3 py-2 text-sm text-foreground focus:outline-none focus:ring-2 focus:ring-primary/50"
              />
            </div>
            <Button
              variant="primary"
              size="sm"
              disabled={isBusy}
              onClick={() => void handleCreateUsers()}
            >
              <Plus size={14} />
              {createUsers.isPending ? 'Erstelle...' : 'Test-User erzeugen'}
            </Button>
            <Button
              variant="secondary"
              size="sm"
              disabled={isBusy || testUsers.length === 0}
              onClick={() => void handleWipeUsers()}
            >
              <Trash2 size={14} />
              Test-User wipen
            </Button>
          </div>
          {testUsers.length > 0 && (
            <details className="text-xs text-muted">
              <summary className="cursor-pointer hover:text-foreground">
                {testUsers.length} Test-User anzeigen
              </summary>
              <div className="mt-2 max-h-40 overflow-y-auto rounded border border-border bg-background/40 p-2">
                {testUsers.map((u) => (
                  <div key={u.discord_id} className="flex justify-between gap-4 py-0.5">
                    <span className="font-mono">{u.discord_id}</span>
                    <span>{u.display_name}</span>
                    <span className="text-muted">{u.rank ?? '-'}</span>
                  </div>
                ))}
              </div>
            </details>
          )}
        </div>
      </Card>

      <Card>
        <div className="space-y-4">
          <div className="flex items-center gap-2">
            <FlaskConical size={16} className="text-primary" />
            <h3 className="font-semibold text-foreground">Test-Turnier erstellen</h3>
          </div>
          <p className="text-xs text-muted">
            Falls nicht genug Test-User vorhanden sind, werden sie automatisch nachgeseedet.
            Das Turnier wird direkt bis zur gewählten Phase advanced, ideal zum Bracket-Test.
          </p>
          <div className="grid gap-3 sm:grid-cols-2">
            <div className="flex flex-col gap-1">
              <label className="text-xs text-muted">Name</label>
              <input
                type="text"
                value={tournamentForm.name}
                onChange={(e) =>
                  setTournamentForm({ ...tournamentForm, name: e.target.value })
                }
                className="rounded-lg border border-border bg-background px-3 py-2 text-sm text-foreground focus:outline-none focus:ring-2 focus:ring-primary/50"
              />
            </div>
            <div className="flex flex-col gap-1">
              <label className="text-xs text-muted">Game-Mode</label>
              <select
                value={tournamentForm.tournament_game_mode}
                onChange={(e) =>
                  setTournamentForm({
                    ...tournamentForm,
                    tournament_game_mode: e.target.value as TournamentGameMode,
                  })
                }
                className="rounded-lg border border-border bg-background px-3 py-2 text-sm text-foreground focus:outline-none focus:ring-2 focus:ring-primary/50"
              >
                {GAME_MODE_OPTIONS.map((m) => (
                  <option key={m.value} value={m.value}>
                    {m.label}
                  </option>
                ))}
              </select>
            </div>
            <div className="flex flex-col gap-1">
              <label className="text-xs text-muted">Team-Größe</label>
              <input
                type="number"
                min={1}
                max={options?.team_size_max}
                value={tournamentForm.team_size}
                onChange={(e) =>
                  setTournamentForm({
                    ...tournamentForm,
                    team_size: Number(e.target.value) || 0,
                  })
                }
                className="rounded-lg border border-border bg-background px-3 py-2 text-sm text-foreground focus:outline-none focus:ring-2 focus:ring-primary/50"
              />
            </div>
            <div className="flex flex-col gap-1">
              <label className="text-xs text-muted">
                {tournamentForm.solo_signups
                  ? `Anzahl Teams (ergibt ${pluralize(tournamentForm.team_size * tournamentForm.num_teams, 'Solo-Spieler', 'Solo-Spieler')})`
                  : 'Anzahl Teams'}
              </label>
              <input
                type="number"
                min={options?.num_teams_min}
                max={options?.num_teams_max}
                value={tournamentForm.num_teams}
                onChange={(e) =>
                  setTournamentForm({
                    ...tournamentForm,
                    num_teams: Number(e.target.value) || 0,
                  })
                }
                className="rounded-lg border border-border bg-background px-3 py-2 text-sm text-foreground focus:outline-none focus:ring-2 focus:ring-primary/50"
              />
            </div>
            <div className="flex flex-col gap-1">
              <label className="text-xs text-muted">Modus</label>
              <select
                value={tournamentForm.mode}
                onChange={(e) =>
                  setTournamentForm({
                    ...tournamentForm,
                    mode: e.target.value as TournamentMode,
                    advance_to: keepAllowed(
                      allowedTargets(options, e.target.value as TournamentMode, tournamentForm.solo_signups ?? false),
                      tournamentForm.advance_to,
                    ),
                  })
                }
                className="rounded-lg border border-border bg-background px-3 py-2 text-sm text-foreground focus:outline-none focus:ring-2 focus:ring-primary/50"
              >
                {MODE_OPTIONS.map((m) => (
                  <option key={m.value} value={m.value}>
                    {m.label}
                  </option>
                ))}
              </select>
            </div>
            <div className="flex flex-col gap-1">
              <label className="text-xs text-muted">Sofort advancen bis</label>
              <select
                value={tournamentForm.advance_to}
                disabled={tournamentForm.solo_signups ?? false}
                onChange={(e) =>
                  setTournamentForm({
                    ...tournamentForm,
                    advance_to: e.target.value as TestAdvance,
                  })
                }
                className="rounded-lg border border-border bg-background px-3 py-2 text-sm text-foreground focus:outline-none focus:ring-2 focus:ring-primary/50"
              >
                {allowedTargets(options, tournamentForm.mode, tournamentForm.solo_signups ?? false).map((target) => (
                  <option key={target} value={target}>
                    {advanceLabel(target, tournamentForm.mode)}
                  </option>
                ))}
              </select>
            </div>
            <label className="flex items-start gap-2 text-sm text-foreground sm:col-span-2">
              <input
                type="checkbox"
                checked={tournamentForm.solo_signups ?? false}
                onChange={(e) =>
                  setTournamentForm({
                    ...tournamentForm,
                    solo_signups: e.target.checked,
                    advance_to: keepAllowed(
                      allowedTargets(options, tournamentForm.mode, e.target.checked),
                      tournamentForm.advance_to,
                    ),
                  })
                }
                className="mt-1"
              />
              <span>
                PUG-Probe: Team-Größe mal Anzahl Teams Spieler, alle nur solo angemeldet und eingecheckt, ohne Teams.
                Das Turnier bleibt im Check-in und erscheint unter „Live“ mit dem Hinweis „Test“.
              </span>
            </label>
          </div>
          <Button
            variant="primary"
            size="sm"
            disabled={isBusy || !tournamentForm.name.trim()}
            onClick={() => void handleCreateTournament()}
          >
            <Plus size={14} />
            {createTournament.isPending ? 'Erstelle...' : 'Test-Turnier erstellen'}
          </Button>
        </div>
      </Card>

      <Card>
        <div className="space-y-4">
          <div className="flex items-center gap-2">
            <Play size={16} className="text-primary" />
            <h3 className="font-semibold text-foreground">
              Aktive Test-Turniere ({testTournaments.length})
            </h3>
          </div>
          {testTournaments.length === 0 ? (
            <p className="text-xs text-muted">Aktuell keine Test-Turniere vorhanden.</p>
          ) : (
            <div className="space-y-2">
              {testTournaments.map((t) => (
                <div
                  key={t.id}
                  className="flex flex-wrap items-center justify-between gap-2 rounded-lg border border-amber-500/20 bg-amber-500/5 p-3"
                >
                  <div className="flex flex-col">
                    <span className="text-sm font-semibold text-foreground">
                      #{t.id} · {t.name}
                    </span>
                    <span className="text-xs text-muted">
                      Status: {t.status} · Game-Mode: {t.tournament_game_mode}
                    </span>
                  </div>
                  {options?.simulatable_statuses.includes(t.status) && (
                    <Button
                      variant="secondary"
                      size="sm"
                      disabled={isBusy}
                      onClick={() => void handleSimulateRound(t.id)}
                    >
                      <Play size={14} />
                      Runde simulieren
                    </Button>
                  )}
                </div>
              ))}
            </div>
          )}
        </div>
      </Card>

      <Card className="border-red-500/30">
        <div className="space-y-3">
          <div className="flex items-center gap-2">
            <AlertTriangle size={16} className="text-red-400" />
            <h3 className="font-semibold text-foreground">Komplett-Wipe</h3>
          </div>
          <p className="text-xs text-muted">
            Löscht ALLE Test-Turniere und ALLE Test-User in einem Rutsch. Produktive
            Turniere bleiben unberührt.
          </p>
          <Button
            variant="secondary"
            size="sm"
            disabled={isBusy}
            onClick={() => void handleWipeAll()}
          >
            <Trash2 size={14} />
            Alles wipen
          </Button>
        </div>
      </Card>
    </div>
  )
}
