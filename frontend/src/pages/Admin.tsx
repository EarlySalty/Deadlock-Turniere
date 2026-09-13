import { useEffect, useMemo, useState } from 'react'
import {
  useAdminTournament,
  useAdminTournaments,
  useDeleteTournament,
} from '@/hooks/useTournament'
import Card from '@/components/ui/Card'
import Badge from '@/components/ui/Badge'
import Button from '@/components/ui/Button'
import LoadingSpinner from '@/components/ui/LoadingSpinner'
import CreateTournamentForm from '@/components/admin/CreateTournamentForm'
import TournamentManager from '@/components/admin/TournamentManager'
import ParticipantManager from '@/components/admin/ParticipantManager'
import CheckinManager from '@/components/admin/CheckinManager'
import MatchAdminPanel from '@/components/admin/MatchAdminPanel'
import MatchEventPanel from '@/components/admin/MatchEventPanel'
import GroupMatchAdminPanel from '@/components/admin/GroupMatchAdminPanel'
import VoiceChannelPanel from '@/components/admin/VoiceChannelPanel'
import TournamentCasterPanel from '@/components/admin/TournamentCasterPanel'
import ArchivedTournamentView from '@/components/admin/ArchivedTournamentView'
import TestModePanel from '@/components/admin/TestModePanel'
import AutomatikPanel from '@/components/admin/automatik/AutomatikPanel'
import ObserverPanel from '@/components/admin/ObserverPanel'
import { AUTOMATIK_COPY } from '@/components/admin/automatik/copy'
import Leitstand from '@/components/admin/Leitstand'
import AdminPhaseNav, {
  defaultPhaseFor,
  type AdminPhase,
} from '@/components/admin/AdminPhaseNav'
import GroupStandings from '@/components/groups/GroupStandings'
import BracketView from '@/components/bracket/BracketView'
import MiniGroupPanel from '@/components/bracket/MiniGroupPanel'
import AutoLobbyButton from '@/components/admin/AutoLobbyButton'
import {
  Archive,
  CalendarClock,
  Crosshair,
  FlaskConical,
  Plus,
  Radio,
  Settings,
  Sparkles,
  Trash2,
  Trophy,
} from 'lucide-react'

type AdminMode = 'live' | 'archive' | 'test' | 'automatik' | 'observer'

const MODE_TABS: { key: AdminMode; label: string; icon: typeof Trophy }[] = [
  { key: 'live', label: 'Live', icon: Radio },
  { key: 'archive', label: 'Archiv', icon: Archive },
  { key: 'test', label: 'Test-Modus', icon: FlaskConical },
  { key: 'automatik', label: AUTOMATIK_COPY.tabLabel, icon: CalendarClock },
  { key: 'observer', label: 'Observer', icon: Crosshair },
]

export default function Admin() {
  const { data: tournaments, isLoading } = useAdminTournaments()
  const [adminMode, setAdminMode] = useState<AdminMode>('live')
  const [selectedLiveId, setSelectedLiveId] = useState<number | null>(null)
  const [selectedArchiveId, setSelectedArchiveId] = useState<number | null>(null)
  const [showCreateForm, setShowCreateForm] = useState(false)
  const [activePhase, setActivePhase] = useState<AdminPhase | null>(null)
  const deleteMutation = useDeleteTournament()

  const liveTournaments = useMemo(
    () =>
      (tournaments ?? []).filter(
        (t) =>
          ['draft', 'registration', 'checkin', 'group_phase', 'bracket'].includes(t.status) &&
          !t.is_test,
      ),
    [tournaments],
  )
  const archivedTournaments = useMemo(
    () =>
      (tournaments ?? []).filter(
        (t) => ['completed', 'archived'].includes(t.status) && !t.is_test,
      ),
    [tournaments],
  )

  const activeLiveTournament = liveTournaments[0] ?? null
  const liveSelectedId =
    (selectedLiveId && liveTournaments.some((t) => t.id === selectedLiveId)
      ? selectedLiveId
      : activeLiveTournament?.id) ?? null
  const archiveSelectedId =
    selectedArchiveId && archivedTournaments.some((t) => t.id === selectedArchiveId)
      ? selectedArchiveId
      : null

  const {
    data: liveDetail,
    isLoading: loadingLiveDetail,
    refetch: refetchLiveDetail,
  } = useAdminTournament(adminMode === 'live' && liveSelectedId ? liveSelectedId : 0)

  const { data: archiveDetail, isLoading: loadingArchiveDetail } = useAdminTournament(
    adminMode === 'archive' && archiveSelectedId ? archiveSelectedId : 0,
  )

  const hasGroups = Boolean(liveDetail?.groups.some((g) => g.matches.length > 0))
  const hasBracket = Boolean(liveDetail?.bracket_matches.length)

  useEffect(() => {
    if (!liveDetail) {
      // eslint-disable-next-line react-hooks/set-state-in-effect
      setActivePhase(null)
      return
    }
    setActivePhase((current) => {
      const fallback = defaultPhaseFor(liveDetail.status, hasGroups, hasBracket)
      return current ?? fallback
    })
  }, [liveDetail, hasGroups, hasBracket])

  useEffect(() => {
    // eslint-disable-next-line react-hooks/set-state-in-effect
    setActivePhase(null)
  }, [liveSelectedId])

  if (isLoading) return <LoadingSpinner />

  const teamCount = liveDetail?.teams.length ?? 0
  const playerCount =
    liveDetail?.teams.reduce((sum, t) => sum + t.members.length, 0) ?? 0
  const matchCount =
    (liveDetail?.bracket_matches.length ?? 0) +
    (liveDetail?.groups.reduce((sum, g) => sum + g.matches.length, 0) ?? 0)
  const groupMatchesStarted = liveDetail
    ? liveDetail.groups.some((g) =>
        g.matches.some(
          (m) => m.status !== 'pending' || m.winner_id !== null || m.played_at !== null,
        ),
      )
    : false
  const canChangeTournamentMode = liveDetail
    ? liveDetail.status === 'draft' ||
      liveDetail.status === 'checkin' ||
      (liveDetail.status === 'group_phase' && !groupMatchesStarted)
    : false
  const canManageParticipants = liveDetail
    ? ['draft', 'registration', 'checkin', 'group_phase', 'bracket'].includes(
        liveDetail.status,
      )
    : false
  const allowManualOverride = liveDetail
    ? ['completed', 'archived'].includes(liveDetail.status)
    : false
  const currentBracketMatchId =
    liveDetail?.bracket_matches.find(
      (m) => !['completed', 'forfeit', 'cancelled'].includes(m.status),
    )?.id ?? null

  const handleDeleteArchived = (id: number, name: string) => {
    if (!window.confirm(`Turnier "${name}" endgültig löschen?`)) return
    deleteMutation.mutate(id)
    if (selectedArchiveId === id) setSelectedArchiveId(null)
  }

  const handleSelectLive = (id: number) => {
    setSelectedLiveId(id)
    setShowCreateForm(false)
  }

  return (
    <div className="space-y-6">
      <header className="flex flex-col gap-2">
        <h1 className="text-2xl font-bold text-foreground">Turnier-Verwaltung</h1>
        <p className="text-sm text-muted">
          Live-Turniere steuern, Archiv einsehen oder im Test-Modus trockene Probeläufe
          fahren.
        </p>
      </header>

      <div role="tablist" className="flex gap-1 border-b border-border">
        {MODE_TABS.map((tab) => {
          const Icon = tab.icon
          const isActive = adminMode === tab.key
          const count =
            tab.key === 'live'
              ? liveTournaments.length
              : tab.key === 'archive'
                ? archivedTournaments.length
                : tab.key === 'test'
                  ? (tournaments ?? []).filter((t) => t.is_test).length
                  : 0
          return (
            <button
              key={tab.key}
              role="tab"
              aria-selected={isActive}
              onClick={() => setAdminMode(tab.key)}
              className={`flex items-center gap-2 border-b-2 px-4 py-2.5 text-sm font-medium transition-colors ${
                isActive
                  ? 'border-primary text-primary'
                  : 'border-transparent text-muted hover:text-foreground'
              }`}
            >
              <Icon size={14} />
              {tab.label}
              <span
                className={`rounded-full px-2 py-0.5 text-[10px] ${
                  isActive ? 'bg-primary/20 text-primary' : 'bg-muted/20 text-muted'
                }`}
              >
                {count}
              </span>
            </button>
          )
        })}
      </div>

      {adminMode === 'live' && (
        <div className="grid gap-6 lg:grid-cols-[260px_1fr]">
          <aside className="lg:sticky lg:top-4 lg:self-start lg:max-h-[calc(100vh-2rem)] lg:overflow-y-auto">
            <Card className="space-y-3 p-4">
              <div className="flex items-center justify-between">
                <h2 className="text-sm font-semibold uppercase tracking-wider text-muted">
                  Aktiv
                </h2>
                <Button
                  variant="primary"
                  size="sm"
                  onClick={() => setShowCreateForm(true)}
                >
                  <Plus size={14} />
                  Neu
                </Button>
              </div>

              {liveTournaments.length === 0 ? (
                <p className="rounded-lg border border-dashed border-border px-3 py-4 text-center text-xs text-muted">
                  Kein aktives Turnier
                </p>
              ) : (
                <ul className="space-y-2">
                  {liveTournaments.map((t) => (
                    <li key={t.id}>
                      <button
                        type="button"
                        onClick={() => handleSelectLive(t.id)}
                        className={`w-full rounded-lg border p-3 text-left transition-colors ${
                          liveSelectedId === t.id
                            ? 'border-primary/60 bg-primary/10'
                            : 'border-border hover:bg-card-hover'
                        }`}
                      >
                        <div className="flex items-center justify-between gap-2">
                          <span className="flex items-center gap-2 truncate font-medium text-foreground">
                            <Trophy size={14} className="shrink-0 text-primary" />
                            <span className="truncate">{t.name}</span>
                          </span>
                          <span className="inline-block h-2 w-2 shrink-0 rounded-full bg-green-400" />
                        </div>
                        <div className="mt-2 flex items-center gap-2">
                          <Badge status={t.status} />
                          <span className="text-[10px] text-muted">
                            {new Date(t.created_at).toLocaleDateString('de-DE')}
                          </span>
                        </div>
                      </button>
                    </li>
                  ))}
                </ul>
              )}
            </Card>
          </aside>

          <main className="min-w-0 space-y-6">
            {showCreateForm ? (
              <section className="space-y-3">
                <div className="flex items-center justify-between">
                  <h2 className="flex items-center gap-2 text-lg font-semibold text-foreground">
                    <Sparkles size={18} className="text-primary" />
                    Neues Turnier anlegen
                  </h2>
                  <button
                    type="button"
                    onClick={() => setShowCreateForm(false)}
                    className="text-xs text-muted hover:text-foreground"
                  >
                    Abbrechen
                  </button>
                </div>
                {activeLiveTournament && (
                  <Card className="p-4">
                    <p className="text-sm text-warning">
                      Solange ein aktives Turnier existiert, blockiert der Server das
                      Anlegen eines weiteren.
                    </p>
                  </Card>
                )}
                <CreateTournamentForm />
              </section>
            ) : loadingLiveDetail && liveSelectedId ? (
              <LoadingSpinner />
            ) : liveDetail && activePhase ? (
              <div className="grid gap-5 lg:grid-cols-[170px_1fr]">
                <AdminPhaseNav
                  status={liveDetail.status}
                  hasGroups={hasGroups}
                  hasBracket={hasBracket}
                  activePhase={activePhase}
                  onChange={setActivePhase}
                />

                <div className="min-w-0 space-y-5">
                  {(liveDetail.status === 'group_phase' ||
                    liveDetail.status === 'bracket') && (
                    <Leitstand tournamentId={liveDetail.id} />
                  )}

                  {activePhase === 'setup' && (
                    <TournamentManager
                      key={`${liveDetail.id}-${liveDetail.updated_at}`}
                      tournament={liveDetail}
                      teamCount={teamCount}
                      playerCount={playerCount}
                      matchCount={matchCount}
                      canChangeTournamentMode={canChangeTournamentMode}
                    />
                  )}

                  {activePhase === 'participants' && canManageParticipants && (
                    <ParticipantManager
                      tournamentId={liveDetail.id}
                      tournamentStatus={liveDetail.status}
                      teamSize={liveDetail.team_size}
                      teams={liveDetail.teams}
                      signups={liveDetail.signups}
                    />
                  )}

                  {activePhase === 'checkin' && liveDetail.status === 'checkin' && (
                    <CheckinManager
                      tournamentId={liveDetail.id}
                      teamSize={liveDetail.team_size}
                      teams={liveDetail.teams}
                      signups={liveDetail.signups}
                    />
                  )}

                  {activePhase === 'group_phase' && hasGroups && (
                    <section className="space-y-4">
                      <header>
                        <h2 className="text-lg font-semibold text-foreground">Gruppenphase</h2>
                        <p className="mt-1 text-sm text-muted">
                          Tabellen einsehen, Gruppenspiele steuern und fehlende Ergebnisse
                          nachtragen.
                        </p>
                      </header>
                      <GroupStandings groups={liveDetail.groups} teams={liveDetail.teams} />
                      <GroupMatchAdminPanel
                        tournamentId={liveDetail.id}
                        groups={liveDetail.groups}
                        teams={liveDetail.teams}
                        onRefresh={() => void refetchLiveDetail()}
                      />
                    </section>
                  )}

                  {activePhase === 'bracket' && hasBracket && (
                    <section className="space-y-4">
                      <header>
                        <h2 className="text-lg font-semibold text-foreground">Matches</h2>
                        <p className="mt-1 text-sm text-muted">
                          Bracket-Übersicht und Match-Steuerung. Winner-Bracket läuft auf
                          Stream, Loser-Bracket parallel.
                        </p>
                      </header>
                      <AutoLobbyButton
                        tournamentId={liveDetail.id}
                        enabled={liveDetail.auto_lobby_enabled}
                      />
                      {liveDetail.mini_groups.length > 0 && (
                        <MiniGroupPanel
                          miniGroups={liveDetail.mini_groups}
                          matches={liveDetail.bracket_matches}
                          teams={liveDetail.teams}
                        />
                      )}
                      <BracketView
                        matches={liveDetail.bracket_matches}
                        teams={liveDetail.teams}
                      />
                      <MatchAdminPanel
                        tournamentId={liveDetail.id}
                        matches={liveDetail.bracket_matches}
                        teams={liveDetail.teams}
                        onRefresh={() => void refetchLiveDetail()}
                        allowManualOverride={allowManualOverride}
                      />
                      <details className="rounded-xl border border-border bg-card">
                        <summary className="cursor-pointer px-4 py-3 text-sm font-medium text-foreground/85">
                          Match-Events (erweitert)
                        </summary>
                        <div className="border-t border-border p-4">
                          <MatchEventPanel
                            tournamentId={liveDetail.id}
                            matches={liveDetail.bracket_matches}
                            teams={liveDetail.teams}
                            onRefresh={() => void refetchLiveDetail()}
                          />
                        </div>
                      </details>
                    </section>
                  )}

                  {activePhase === 'voice' && (
                    <section className="space-y-4">
                      <header>
                        <h2 className="text-lg font-semibold text-foreground">
                          Voice & Caster
                        </h2>
                        <p className="mt-1 text-sm text-muted">
                          Caster-Liste fürs gesamte Turnier pflegen und Voice-Channel-Splits
                          für laufende Matches steuern.
                        </p>
                      </header>
                      <TournamentCasterPanel tournamentId={liveDetail.id} />
                      {hasBracket && (
                        <VoiceChannelPanel
                          tournamentId={liveDetail.id}
                          currentMatchId={currentBracketMatchId}
                        />
                      )}
                    </section>
                  )}
                </div>
              </div>
            ) : (
              <Card className="p-8 text-center">
                <Settings size={32} className="mx-auto mb-3 text-muted" />
                <p className="text-muted">
                  Kein Turnier ausgewählt — links auswählen oder ein neues anlegen.
                </p>
              </Card>
            )}
          </main>
        </div>
      )}

      {adminMode === 'archive' && (
        <div className="grid gap-6 lg:grid-cols-[320px_1fr]">
          <aside className="lg:sticky lg:top-4 lg:self-start lg:max-h-[calc(100vh-2rem)] lg:overflow-y-auto">
            <Card className="space-y-3 p-4">
              <h2 className="flex items-center gap-2 text-sm font-semibold uppercase tracking-wider text-muted">
                <Archive size={14} />
                Archiv ({archivedTournaments.length})
              </h2>
              {archivedTournaments.length === 0 ? (
                <p className="rounded-lg border border-dashed border-border px-3 py-4 text-center text-xs text-muted">
                  Keine archivierten Turniere
                </p>
              ) : (
                <ul className="space-y-2">
                  {archivedTournaments.map((t) => (
                    <li key={t.id}>
                      <div
                        className={`group rounded-lg border transition-colors ${
                          archiveSelectedId === t.id
                            ? 'border-primary/60 bg-primary/10'
                            : 'border-border hover:bg-card-hover'
                        }`}
                      >
                        <button
                          type="button"
                          onClick={() => setSelectedArchiveId(t.id)}
                          className="block w-full p-3 text-left"
                        >
                          <div className="truncate text-sm font-medium text-foreground">
                            {t.name}
                          </div>
                          <div className="mt-1 flex items-center gap-2">
                            <Badge status={t.status} />
                            <span className="text-[10px] text-muted">
                              {new Date(t.created_at).toLocaleDateString('de-DE')}
                            </span>
                          </div>
                        </button>
                        <div className="border-t border-border/40 px-2 py-1.5 opacity-0 transition-opacity group-hover:opacity-100">
                          <button
                            type="button"
                            onClick={() => handleDeleteArchived(t.id, t.name)}
                            disabled={deleteMutation.isPending}
                            className="flex w-full items-center justify-center gap-1.5 rounded px-2 py-1 text-[11px] text-red-300 hover:bg-red-500/10 disabled:opacity-50"
                          >
                            <Trash2 size={11} />
                            Endgültig löschen
                          </button>
                        </div>
                      </div>
                    </li>
                  ))}
                </ul>
              )}
            </Card>
          </aside>

          <main className="min-w-0 space-y-6">
            {!archiveSelectedId ? (
              <Card className="p-8 text-center">
                <Archive size={32} className="mx-auto mb-3 text-muted" />
                <p className="text-muted">
                  Wähle links ein archiviertes Turnier, um Bracket, Teams und Caster-Liste
                  einzusehen. Read-Only — keine Bearbeitung möglich.
                </p>
              </Card>
            ) : loadingArchiveDetail ? (
              <LoadingSpinner />
            ) : archiveDetail ? (
              <ArchivedTournamentView tournament={archiveDetail} />
            ) : (
              <Card className="p-8 text-center">
                <p className="text-muted">Turnier konnte nicht geladen werden.</p>
              </Card>
            )}
          </main>
        </div>
      )}

      {adminMode === 'test' && <TestModePanel />}
      {adminMode === 'automatik' && <AutomatikPanel />}
      {adminMode === 'observer' && <ObserverPanel />}
    </div>
  )
}
