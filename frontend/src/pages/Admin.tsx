import { useEffect, useMemo, useState } from 'react'
import {
  useAdminTournament,
  useAdminTournaments,
  useDeleteTournament,
} from '@/hooks/useTournament'
import Card from '@/components/ui/Card'
import LoadingSpinner from '@/components/ui/LoadingSpinner'
import CreateTournamentForm from '@/components/admin/CreateTournamentForm'
import TournamentManager from '@/components/admin/TournamentManager'
import ParticipantManager from '@/components/admin/ParticipantManager'
import CheckinManager from '@/components/admin/CheckinManager'
import MatchAdminPanel from '@/components/admin/MatchAdminPanel'
import MatchEventPanel from '@/components/admin/MatchEventPanel'
import GroupMatchAdminPanel from '@/components/admin/GroupMatchAdminPanel'
import VoiceChannelPanel from '@/components/admin/VoiceChannelPanel'
import AdminTournamentSidebar from '@/components/admin/AdminTournamentSidebar'
import AdminPhaseNav, {
  defaultPhaseFor,
  type AdminPhase,
} from '@/components/admin/AdminPhaseNav'
import GroupStandings from '@/components/groups/GroupStandings'
import BracketView from '@/components/bracket/BracketView'
import MiniGroupPanel from '@/components/bracket/MiniGroupPanel'
import AutoLobbyButton from '@/components/admin/AutoLobbyButton'
import { Settings, Sparkles } from 'lucide-react'

type BracketSubTab = 'matches' | 'events' | 'overview'

export default function Admin() {
  const { data: tournaments, isLoading } = useAdminTournaments()
  const [selectedTournamentId, setSelectedTournamentId] = useState<number | null>(null)
  const [showCreateForm, setShowCreateForm] = useState(false)
  const [activePhase, setActivePhase] = useState<AdminPhase | null>(null)
  const [bracketSubTab, setBracketSubTab] = useState<BracketSubTab>('overview')
  const deleteMutation = useDeleteTournament()

  const activeTournament = useMemo(
    () =>
      tournaments?.find((tournament) =>
        ['draft', 'registration', 'checkin', 'group_phase', 'bracket'].includes(tournament.status),
      ) ?? null,
    [tournaments],
  )

  const archived = useMemo(
    () =>
      (tournaments ?? []).filter((tournament) =>
        ['completed', 'archived'].includes(tournament.status),
      ),
    [tournaments],
  )

  const fallbackTournamentId =
    activeTournament?.id ?? archived[0]?.id ?? tournaments?.[0]?.id ?? null
  const resolvedSelectedTournamentId =
    (selectedTournamentId &&
    tournaments?.some((tournament) => tournament.id === selectedTournamentId)
      ? selectedTournamentId
      : fallbackTournamentId) ?? null

  const {
    data: selectedDetail,
    isLoading: isLoadingDetail,
    refetch: refetchSelectedDetail,
  } = useAdminTournament(resolvedSelectedTournamentId ?? 0)

  const hasGroups = Boolean(selectedDetail?.groups.some((group) => group.matches.length > 0))
  const hasBracket = Boolean(selectedDetail?.bracket_matches.length)

  // Phase auto-anwählen wenn Turnier wechselt oder Daten reinkommen
  useEffect(() => {
    if (!selectedDetail) {
      setActivePhase(null)
      return
    }
    setActivePhase((current) => {
      const fallback = defaultPhaseFor(selectedDetail.status, hasGroups, hasBracket)
      return current ?? fallback
    })
  }, [selectedDetail, hasGroups, hasBracket])

  // Wenn ein anderes Turnier selektiert wird, Phase zurücksetzen
  useEffect(() => {
    setActivePhase(null)
  }, [resolvedSelectedTournamentId])

  if (isLoading) return <LoadingSpinner />

  const teamCount = selectedDetail?.teams.length ?? 0
  const playerCount =
    selectedDetail?.teams.reduce((sum, team) => sum + team.members.length, 0) ?? 0
  const matchCount =
    (selectedDetail?.bracket_matches.length ?? 0) +
    (selectedDetail?.groups.reduce((sum, group) => sum + group.matches.length, 0) ?? 0)
  const groupMatchesStarted = selectedDetail
    ? selectedDetail.groups.some((group) =>
        group.matches.some(
          (match) =>
            match.status !== 'pending' || match.winner_id !== null || match.played_at !== null,
        ),
      )
    : false
  const canChangeTournamentMode = selectedDetail
    ? selectedDetail.status === 'draft' ||
      selectedDetail.status === 'checkin' ||
      (selectedDetail.status === 'group_phase' && !groupMatchesStarted)
    : false
  const canManageParticipants = selectedDetail
    ? ['draft', 'registration', 'checkin', 'group_phase', 'bracket'].includes(
        selectedDetail.status,
      )
    : false
  const allowManualOverride = selectedDetail
    ? ['completed', 'archived'].includes(selectedDetail.status)
    : false
  const currentBracketMatchId =
    selectedDetail?.bracket_matches.find(
      (match) => !['completed', 'forfeit', 'cancelled'].includes(match.status),
    )?.id ?? null

  const handleDeleteArchived = (id: number, name: string) => {
    if (!window.confirm(`Turnier "${name}" endgültig löschen?`)) return
    deleteMutation.mutate(id)
  }

  const handleSelectTournament = (id: number) => {
    setSelectedTournamentId(id)
    setShowCreateForm(false)
  }

  const handleCreateClick = () => {
    setShowCreateForm(true)
  }

  return (
    <div className="space-y-6">
      <header className="flex flex-col gap-2">
        <h1 className="text-2xl font-bold text-foreground">Turnier-Verwaltung</h1>
        <p className="text-sm text-muted">
          Wähle links ein Turnier und navigiere oben zwischen Setup, Teilnehmern,
          Check-in, Gruppenphase, Bracket und Voice/Caster-Steuerung.
        </p>
      </header>

      <div className="grid gap-6 lg:grid-cols-[260px_1fr]">
        <div className="lg:sticky lg:top-4 lg:self-start lg:max-h-[calc(100vh-2rem)] lg:overflow-y-auto">
          <AdminTournamentSidebar
          activeTournament={activeTournament}
          archivedTournaments={archived}
          selectedTournamentId={resolvedSelectedTournamentId}
          onSelectTournament={handleSelectTournament}
          onCreateClick={handleCreateClick}
          onDeleteArchived={handleDeleteArchived}
          deleteDisabled={deleteMutation.isPending}
        />
        </div>

        <main className="min-w-0 space-y-6">
          {showCreateForm ? (
            <section className="space-y-3">
              <div className="flex items-center justify-between">
                <h2 className="flex items-center gap-2 text-lg font-semibold text-foreground">
                  <Sparkles size={18} className="text-primary" />
                  Neues Turnier anlegen
                </h2>
                {selectedDetail && (
                  <button
                    type="button"
                    onClick={() => setShowCreateForm(false)}
                    className="text-xs text-muted hover:text-foreground"
                  >
                    Zurück zum ausgewählten Turnier
                  </button>
                )}
              </div>
              {activeTournament && (
                <Card className="p-4">
                  <p className="text-sm text-warning">
                    Solange ein aktives Turnier existiert, blockiert der Server das Anlegen
                    eines weiteren.
                  </p>
                </Card>
              )}
              <CreateTournamentForm />
            </section>
          ) : isLoadingDetail && resolvedSelectedTournamentId ? (
            <LoadingSpinner />
          ) : selectedDetail && activePhase ? (
            <>
              <AdminPhaseNav
                status={selectedDetail.status}
                hasGroups={hasGroups}
                hasBracket={hasBracket}
                activePhase={activePhase}
                onChange={setActivePhase}
              />

              {activePhase === 'setup' && (
                <TournamentManager
                  key={`${selectedDetail.id}-${selectedDetail.updated_at}`}
                  tournament={selectedDetail}
                  teamCount={teamCount}
                  playerCount={playerCount}
                  matchCount={matchCount}
                  canChangeTournamentMode={canChangeTournamentMode}
                />
              )}

              {activePhase === 'participants' && canManageParticipants && (
                <ParticipantManager
                  tournamentId={selectedDetail.id}
                  tournamentStatus={selectedDetail.status}
                  teamSize={selectedDetail.team_size}
                  teams={selectedDetail.teams}
                  signups={selectedDetail.signups}
                />
              )}

              {activePhase === 'checkin' && selectedDetail.status === 'checkin' && (
                <CheckinManager
                  tournamentId={selectedDetail.id}
                  teamSize={selectedDetail.team_size}
                  teams={selectedDetail.teams}
                  signups={selectedDetail.signups}
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
                  <GroupStandings
                    groups={selectedDetail.groups}
                    teams={selectedDetail.teams}
                  />
                  <GroupMatchAdminPanel
                    tournamentId={selectedDetail.id}
                    groups={selectedDetail.groups}
                    teams={selectedDetail.teams}
                    onRefresh={() => void refetchSelectedDetail()}
                  />
                </section>
              )}

              {activePhase === 'bracket' && hasBracket && (
                <section className="space-y-4">
                  <header>
                    <h2 className="text-lg font-semibold text-foreground">Bracket</h2>
                    <p className="mt-1 text-sm text-muted">
                      Live-Übersicht, Match-Steuerung und Match-Events ohne Hexenwerk.
                    </p>
                  </header>

                  <div role="tablist" className="flex border-b border-border">
                    <button
                      role="tab"
                      aria-selected={bracketSubTab === 'overview'}
                      onClick={() => setBracketSubTab('overview')}
                      className={`border-b-2 px-4 py-2.5 text-sm font-medium transition-colors ${
                        bracketSubTab === 'overview'
                          ? 'border-primary text-primary'
                          : 'border-transparent text-muted hover:text-foreground'
                      }`}
                    >
                      Übersicht
                    </button>
                    <button
                      role="tab"
                      aria-selected={bracketSubTab === 'matches'}
                      onClick={() => setBracketSubTab('matches')}
                      className={`border-b-2 px-4 py-2.5 text-sm font-medium transition-colors ${
                        bracketSubTab === 'matches'
                          ? 'border-primary text-primary'
                          : 'border-transparent text-muted hover:text-foreground'
                      }`}
                    >
                      Match-Steuerung
                    </button>
                    <button
                      role="tab"
                      aria-selected={bracketSubTab === 'events'}
                      onClick={() => setBracketSubTab('events')}
                      className={`border-b-2 px-4 py-2.5 text-sm font-medium transition-colors ${
                        bracketSubTab === 'events'
                          ? 'border-primary text-primary'
                          : 'border-transparent text-muted hover:text-foreground'
                      }`}
                    >
                      Match-Events
                    </button>
                  </div>

                  {bracketSubTab === 'overview' && (
                    <div className="space-y-4">
                      {selectedDetail.mini_groups.length > 0 && (
                        <MiniGroupPanel
                          miniGroups={selectedDetail.mini_groups}
                          matches={selectedDetail.bracket_matches}
                          teams={selectedDetail.teams}
                        />
                      )}
                      <BracketView
                        matches={selectedDetail.bracket_matches}
                        teams={selectedDetail.teams}
                      />
                    </div>
                  )}

                  {bracketSubTab === 'matches' && (
                    <div className="space-y-4">
                      <AutoLobbyButton
                        tournamentId={selectedDetail.id}
                        enabled={selectedDetail.auto_lobby_enabled}
                      />
                      <MatchAdminPanel
                        tournamentId={selectedDetail.id}
                        matches={selectedDetail.bracket_matches}
                        teams={selectedDetail.teams}
                        onRefresh={() => void refetchSelectedDetail()}
                        allowManualOverride={allowManualOverride}
                      />
                    </div>
                  )}

                  {bracketSubTab === 'events' && (
                    <MatchEventPanel
                      tournamentId={selectedDetail.id}
                      matches={selectedDetail.bracket_matches}
                      teams={selectedDetail.teams}
                      onRefresh={() => void refetchSelectedDetail()}
                    />
                  )}
                </section>
              )}

              {activePhase === 'voice' && hasBracket && (
                <section className="space-y-4">
                  <header>
                    <h2 className="text-lg font-semibold text-foreground">
                      Voice & Caster
                    </h2>
                    <p className="mt-1 text-sm text-muted">
                      Voice-Channel-Splits und Caster-Verwaltung für laufende Matches.
                    </p>
                  </header>
                  <VoiceChannelPanel
                    tournamentId={selectedDetail.id}
                    currentMatchId={currentBracketMatchId}
                  />
                </section>
              )}
            </>
          ) : (
            <Card className="p-8 text-center">
              <Settings size={32} className="mx-auto mb-3 text-muted" />
              <p className="text-muted">
                Wähle links ein Turnier, um Details zu öffnen — oder lege ein neues an.
              </p>
            </Card>
          )}
        </main>
      </div>
    </div>
  )
}
