import { useEffect, useMemo, useRef, useState } from 'react'
import {
  useAdminTournament,
  useAdminTournaments,
  useDeleteTournament,
} from '@/hooks/useTournament'
import ActionError from '@/components/ui/ActionError'
import Card from '@/components/ui/Card'
import Badge from '@/components/ui/Badge'
import Button from '@/components/ui/Button'
import LoadingSpinner from '@/components/ui/LoadingSpinner'
import CreateTournamentForm from '@/components/admin/CreateTournamentForm'
import TournamentManager from '@/components/admin/TournamentManager'
import ParticipantManager from '@/components/admin/ParticipantManager'
import TeamBuilderPanel from '@/components/admin/TeamBuilderPanel'
import { ParticipantMarksContext } from '@/components/admin/participantMarksContext'
import AdminWorkflowPanel from '@/components/admin/AdminWorkflowPanel'
import {
  CollapsedTournamentRow,
  TournamentSidebar,
  TournamentSummaryCard,
} from '@/components/admin/TournamentOverview'
import { sortByStart } from '@/lib/adminTournaments'
import CheckinManager from '@/components/admin/CheckinManager'
import MatchAdminPanel from '@/components/admin/MatchAdminPanel'
import MatchEventPanel from '@/components/admin/MatchEventPanel'
import GroupMatchAdminPanel from '@/components/admin/GroupMatchAdminPanel'
import VoiceChannelPanel from '@/components/admin/VoiceChannelPanel'
import TournamentCasterPanel from '@/components/admin/TournamentCasterPanel'
import ArchivedTournamentView from '@/components/admin/ArchivedTournamentView'
import {
  templateFromTournament,
  type CreateTemplate,
} from '@/components/admin/settings/createFormValues'
import TestModePanel from '@/components/admin/TestModePanel'
import AutomatikPanel from '@/components/admin/automatik/AutomatikPanel'
import ObserverPanel from '@/components/admin/ObserverPanel'
import { AUTOMATIK_COPY } from '@/components/admin/automatik/copy'
import Leitstand from '@/components/admin/Leitstand'
import AdminPhaseNav from '@/components/admin/AdminPhaseNav'
import { defaultPhaseFor, type AdminPhase } from '@/components/admin/adminPhase'
import GroupStandings from '@/components/groups/GroupStandings'
import GroupMatchList from '@/components/groups/GroupMatchList'
import LeaguePlanPanel from '@/components/admin/LeaguePlanPanel'
import { LEAGUE_LABEL, isLeague } from '@/lib/league'
import BracketView from '@/components/bracket/BracketView'
import MiniGroupPanel from '@/components/bracket/MiniGroupPanel'
import AutoLobbyButton from '@/components/admin/AutoLobbyButton'
import { effectiveBracketFormat } from '@/lib/bracketFormat'
import { bracketHasResults } from '@/lib/adminWorkflow'
import {
  Archive,
  CalendarClock,
  Crosshair,
  FlaskConical,
  Plus,
  Radio,
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
  const { data: tournaments, isLoading, isError: tournamentsError, refetch: refetchTournaments } = useAdminTournaments()
  const [adminMode, setAdminMode] = useState<AdminMode>('live')
  const [selectedLiveId, setSelectedLiveId] = useState<number | null>(null)
  const [selectedArchiveId, setSelectedArchiveId] = useState<number | null>(null)
  const [showCreateForm, setShowCreateForm] = useState(false)
  const [createTemplate, setCreateTemplate] = useState<CreateTemplate | null>(null)
  const [createFormKey, setCreateFormKey] = useState(0)
  const [activePhase, setActivePhase] = useState<AdminPhase | null>(null)
  const [finishedLiveId, setFinishedLiveId] = useState<number | null>(null)
  const [managerNotice, setManagerNotice] = useState<{ tournamentId: number; text: string } | null>(null)
  const deleteMutation = useDeleteTournament()

  const liveTournaments = useMemo(
    () =>
      sortByStart(
        (tournaments ?? []).filter(
          (t) =>
            ['draft', 'registration', 'checkin', 'group_phase', 'bracket'].includes(t.status) ||
            (t.id === finishedLiveId && t.status === 'completed'),
        ),
      ).sort((a, b) => Number(a.is_test) - Number(b.is_test)),
    [tournaments, finishedLiveId],
  )
  const archivedTournaments = useMemo(
    () =>
      (tournaments ?? []).filter(
        (t) => ['completed', 'archived'].includes(t.status) && !t.is_test,
      ),
    [tournaments],
  )

  const liveSelectedId =
    (selectedLiveId && liveTournaments.some((t) => t.id === selectedLiveId)
      ? selectedLiveId
      : liveTournaments[0]?.id) ?? null
  const selectedSummary = liveTournaments.find((t) => t.id === liveSelectedId) ?? null
  const otherLiveTournaments = liveTournaments.filter((t) => t.id !== liveSelectedId)
  const archiveSelectedId =
    selectedArchiveId && archivedTournaments.some((t) => t.id === selectedArchiveId)
      ? selectedArchiveId
      : null

  const {
    data: liveDetail,
    isLoading: loadingLiveDetail,
    error: liveDetailError,
    refetch: refetchLiveDetail,
  } = useAdminTournament(adminMode === 'live' && liveSelectedId ? liveSelectedId : 0)

  const { data: archiveDetail, isLoading: loadingArchiveDetail } = useAdminTournament(
    adminMode === 'archive' && archiveSelectedId ? archiveSelectedId : 0,
  )

  const participantMarks = useMemo(
    () => ({
      guestIds: new Set(liveDetail?.guest_ids ?? []),
      duplicateNameIds: new Set(liveDetail?.duplicate_name_ids ?? []),
    }),
    [liveDetail?.guest_ids, liveDetail?.duplicate_name_ids],
  )
  const hasGroups = Boolean(liveDetail?.groups.some((g) => g.matches.length > 0))
  const hasBracket = Boolean(liveDetail?.bracket_matches.length)
  const liveLeague = isLeague(liveDetail?.tournament_mode)

  const phaseTournamentId = useRef<number | null>(null)

  useEffect(() => {
    if (!liveDetail) {
      // eslint-disable-next-line react-hooks/set-state-in-effect
      setActivePhase(null)
      return
    }
    const switched = phaseTournamentId.current !== liveDetail.id
    phaseTournamentId.current = liveDetail.id
    setActivePhase((current) => {
      const fallback = defaultPhaseFor(liveDetail.status, hasGroups, hasBracket)
      return switched ? fallback : current ?? fallback
    })
  }, [liveDetail, hasGroups, hasBracket])

  if (isLoading) return <LoadingSpinner />
  if (tournamentsError && !tournaments) {
    return (
      <Card className="space-y-4 p-6">
        <h1 className="text-lg font-semibold text-foreground">Turnier-Verwaltung nicht erreichbar</h1>
        <p className="text-sm text-muted">Die Turnierliste konnte nicht geladen werden. Bitte die Verbindung prüfen und erneut versuchen.</p>
        <Button variant="secondary" size="sm" onClick={() => void refetchTournaments()}>
          Erneut laden
        </Button>
      </Card>
    )
  }

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
    ? ['bracket', 'completed', 'archived'].includes(liveDetail.status)
    : false
  const allowResultReset = liveDetail?.status === 'bracket'
  const currentBracketMatchId =
    liveDetail?.bracket_matches.find(
      (m) => !['completed', 'forfeit', 'cancelled'].includes(m.status),
    )?.id ?? null

  const handleDeleteArchived = (id: number, name: string) => {
    if (!window.confirm(`Turnier "${name}" endgültig löschen?`)) return
    deleteMutation.mutate(id)
    if (selectedArchiveId === id) setSelectedArchiveId(null)
  }

  const openCreateForm = (template: CreateTemplate | null) => {
    setCreateTemplate(template)
    setCreateFormKey((key) => key + 1)
    setShowCreateForm(true)
    setAdminMode('live')
  }

  const handleSelectLive = (id: number) => {
    setSelectedLiveId(id)
    setShowCreateForm(false)
    window.scrollTo({ top: 0, behavior: 'smooth' })
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

      <div role="tablist" className="flex flex-wrap gap-1 border-b border-border">
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
        <div className="grid min-w-0 grid-cols-1 gap-6 xl:grid-cols-[260px_minmax(0,1fr)]">
          <aside className="xl:sticky xl:top-20 xl:self-start xl:max-h-[calc(100vh-6rem)] xl:overflow-y-auto">
            <TournamentSidebar
              tournaments={liveTournaments}
              selectedId={liveSelectedId}
              creating={showCreateForm}
              onSelect={handleSelectLive}
              onCreate={() => openCreateForm(null)}
            />
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
                <CreateTournamentForm key={createFormKey} template={createTemplate ?? undefined} />
              </section>
            ) : loadingLiveDetail && liveSelectedId ? (
              <LoadingSpinner />
            ) : liveDetail && activePhase ? (
              <>
                {liveDetail.id === finishedLiveId && liveDetail.status === 'completed' && (
                  <div
                    role="status"
                    className="flex items-center gap-2 rounded-lg border border-green-500/20 bg-green-500/10 p-3 text-sm text-green-400"
                  >
                    <Trophy size={16} />
                    <span>Turnier abgeschlossen. Ergebnisse und Platzierungen sind gespeichert.</span>
                  </div>
                )}
                {selectedSummary && <TournamentSummaryCard tournament={selectedSummary} />}
                <AdminWorkflowPanel
                  tournament={liveDetail}
                  hasGroups={hasGroups}
                  hasBracket={hasBracket}
                  onNavigate={setActivePhase}
                />
                <AdminPhaseNav
                  status={liveDetail.status}
                  hasGroups={hasGroups}
                  hasBracket={hasBracket}
                  league={liveLeague}
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
                        bracketHasResults={bracketHasResults(liveDetail)}
                        onFinished={() => setFinishedLiveId(liveDetail.id)}
                        successMessage={managerNotice?.tournamentId === liveDetail.id ? managerNotice.text : ''}
                        onSuccessMessage={(text) => setManagerNotice({ tournamentId: liveDetail.id, text })}
                      />
                    )}

                    {activePhase === 'participants' && canManageParticipants && liveDetail.team_size > 1 && (
                      <ParticipantMarksContext.Provider value={participantMarks}>
                        <TeamBuilderPanel
                          key={liveDetail.id}
                          tournamentId={liveDetail.id}
                          teamSize={liveDetail.team_size}
                          teams={liveDetail.teams}
                          signups={liveDetail.signups}
                          canAddPlayers={liveDetail.registration_open}
                        />
                      </ParticipantMarksContext.Provider>
                    )}

                    {activePhase === 'participants' && canManageParticipants && liveDetail.team_size > 1 && (
                      <details className="min-w-0 rounded-xl border border-border bg-card/40">
                        <summary className="cursor-pointer px-4 py-3 text-sm font-medium text-foreground hover:text-primary">
                          Weitere Teamaktionen (Solo-Balance, Anmeldungen, Löschen, Recruiting, Ersatzspieler)
                        </summary>
                        <div className="min-w-0 border-t border-border p-3 sm:p-4">
                          <ParticipantManager
                            tournamentId={liveDetail.id}
                            replacementsOpen={liveDetail.replacements_open}
                            teamSize={liveDetail.team_size}
                            teams={liveDetail.teams}
                            signups={liveDetail.signups}
                            soloOnly={liveDetail.invite_mode === 'solo_only'}
                          />
                        </div>
                      </details>
                    )}

                    {activePhase === 'participants' && canManageParticipants && liveDetail.team_size <= 1 && (
                      <ParticipantManager
                        tournamentId={liveDetail.id}
                        replacementsOpen={liveDetail.replacements_open}
                        teamSize={liveDetail.team_size}
                        teams={liveDetail.teams}
                        signups={liveDetail.signups}
                        soloOnly={liveDetail.invite_mode === 'solo_only'}
                      />
                    )}

                    {activePhase === 'checkin' && liveDetail.status === 'checkin' && (
                      <ParticipantMarksContext.Provider value={participantMarks}>
                        <CheckinManager
                          tournamentId={liveDetail.id}
                          teamSize={liveDetail.team_size}
                          teams={liveDetail.teams}
                          signups={liveDetail.signups}
                          league={liveLeague}
                        />
                      </ParticipantMarksContext.Provider>
                    )}

                    {activePhase === 'group_phase' && hasGroups && liveLeague && (
                      <section className="space-y-4">
                        <header>
                          <h2 className="text-lg font-semibold text-foreground">{LEAGUE_LABEL}</h2>
                          <p className="mt-1 text-sm text-muted">
                            Tabelle und Spielplan. Kein Bracket: am Ende entscheidet die Tabelle.
                          </p>
                        </header>
                        <GroupStandings groups={liveDetail.groups} teams={liveDetail.teams} league />
                        {liveDetail.status === 'group_phase' ? (
                          <LeaguePlanPanel
                            tournamentId={liveDetail.id}
                            groups={liveDetail.groups}
                            teams={liveDetail.teams}
                            autoLobby={liveDetail.auto_lobby_enabled}
                            countsForLeaderboard={!liveDetail.exclude_from_leaderboard}
                            onRefresh={() => void refetchLiveDetail()}
                          />
                        ) : (
                          <GroupMatchList groups={liveDetail.groups} teams={liveDetail.teams} league />
                        )}
                      </section>
                    )}

                    {activePhase === 'group_phase' && hasGroups && !liveLeague && (
                      <section className="space-y-4">
                        <header>
                          <h2 className="text-lg font-semibold text-foreground">Gruppenphase</h2>
                          <p className="mt-1 text-sm text-muted">
                            Tabellen einsehen, Gruppenspiele steuern und fehlende Ergebnisse
                            nachtragen.
                          </p>
                        </header>
                        <GroupStandings groups={liveDetail.groups} teams={liveDetail.teams} admin />
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
                            {effectiveBracketFormat(liveDetail) === 'double_elimination'
                              ? 'Bracket-Übersicht und Match-Steuerung. Winner-Bracket läuft auf Stream, Loser-Bracket parallel.'
                              : 'Bracket-Übersicht und Match-Steuerung im K.-o.-System.'}
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
                          format={effectiveBracketFormat(liveDetail)}
                        />
                        <MatchAdminPanel
                          tournamentId={liveDetail.id}
                          matches={liveDetail.bracket_matches}
                          teams={liveDetail.teams}
                          onRefresh={() => void refetchLiveDetail()}
                          allowManualOverride={allowManualOverride}
                          allowResultReset={allowResultReset}
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
                {otherLiveTournaments.length > 0 && (
                  <section className="space-y-2">
                    <h2 className="text-xs font-semibold uppercase tracking-wider text-muted">
                      Weitere geplante Turniere
                    </h2>
                    {otherLiveTournaments.map((t) => (
                      <CollapsedTournamentRow key={t.id} tournament={t} onSelect={handleSelectLive} />
                    ))}
                  </section>
                )}
              </>
            ) : liveDetailError && !liveDetail ? (
              <Card className="p-8 text-center">
                <p className="text-sm text-red-300">
                  Turnier konnte nicht geladen werden: {liveDetailError.message}
                </p>
              </Card>
            ) : liveTournaments.length === 0 ? (
              <Card className="p-8 text-center">
                <Trophy size={32} className="mx-auto mb-3 text-muted" />
                <p className="text-muted">Gerade ist kein Turnier geplant.</p>
                <Button variant="primary" size="sm" className="mt-4" onClick={() => openCreateForm(null)}>
                  <Plus size={14} />
                  Turnier erstellen
                </Button>
              </Card>
            ) : (
              <LoadingSpinner />
            )}
          </main>
        </div>
      )}

      {adminMode === 'archive' && (
        <div className="grid gap-6 lg:grid-cols-[320px_1fr]">
          <aside className="lg:sticky lg:top-20 lg:self-start lg:max-h-[calc(100vh-6rem)] lg:overflow-y-auto">
            <Card className="space-y-3 p-4">
              <h2 className="flex items-center gap-2 text-sm font-semibold uppercase tracking-wider text-muted">
                <Archive size={14} />
                Archiv ({archivedTournaments.length})
              </h2>
              <ActionError actions={[deleteMutation]} />
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
                            <Badge status={t.status} mode={t.tournament_mode} />
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
                  einzusehen. Nur lesen, keine Bearbeitung möglich.
                </p>
              </Card>
            ) : loadingArchiveDetail ? (
              <LoadingSpinner />
            ) : archiveDetail ? (
              <ArchivedTournamentView
                tournament={archiveDetail}
                onUseAsTemplate={() => openCreateForm(templateFromTournament(archiveDetail))}
              />
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
