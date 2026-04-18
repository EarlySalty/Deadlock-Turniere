import { useState } from 'react'
import { useAdminTournament, useAdminTournaments, useDeleteTournament } from '@/hooks/useTournament'
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
import { Archive, Plus, Settings, Trash2 } from 'lucide-react'

type AdminTab = 'erstellen' | 'verwalten'
type BracketAdminTab = 'matches' | 'events'

export default function Admin() {
  const { data: tournaments, isLoading } = useAdminTournaments()
  const [activeTab, setActiveTab] = useState<AdminTab>('verwalten')
  const [bracketAdminTab, setBracketAdminTab] = useState<BracketAdminTab>('matches')
  const [selectedTournamentId, setSelectedTournamentId] = useState<number | null>(null)
  const deleteMutation = useDeleteTournament()

  const activeTournament = tournaments?.find((tournament) =>
    ['draft', 'registration', 'checkin', 'group_phase', 'bracket'].includes(tournament.status)
  )
  const archived = tournaments?.filter((tournament) =>
    ['completed', 'archived'].includes(tournament.status)
  ) ?? []

  const fallbackTournamentId = activeTournament?.id ?? archived[0]?.id ?? tournaments?.[0]?.id ?? null
  const resolvedSelectedTournamentId = (
    selectedTournamentId && tournaments?.some((tournament) => tournament.id === selectedTournamentId)
      ? selectedTournamentId
      : fallbackTournamentId
  ) ?? null

  const { data: selectedDetail, isLoading: isLoadingDetail, refetch: refetchSelectedDetail } = useAdminTournament(resolvedSelectedTournamentId ?? 0)

  if (isLoading) return <LoadingSpinner />

  const selectedTournament = tournaments?.find((tournament) => tournament.id === resolvedSelectedTournamentId) ?? null
  const teamCount = selectedDetail?.teams.length ?? 0
  const playerCount = selectedDetail?.teams.reduce((sum, team) => sum + team.members.length, 0) ?? 0
  const matchCount =
    (selectedDetail?.bracket_matches.length ?? 0) +
    (selectedDetail?.groups.reduce((sum, group) => sum + group.matches.length, 0) ?? 0)
  const canManageParticipants = selectedDetail
    ? ['draft', 'registration', 'checkin', 'group_phase', 'bracket'].includes(selectedDetail.status)
    : false
  const showGroupMatchAdmin = Boolean(
    selectedDetail?.groups.some((group) => group.matches.length > 0),
  )
  const showBracketAdmin = Boolean(selectedDetail?.bracket_matches.length)
  const allowManualOverride = selectedDetail
    ? ['completed', 'archived'].includes(selectedDetail.status)
    : false
  const currentBracketMatchId = selectedDetail?.bracket_matches.find(
    (match) => !['completed', 'forfeit', 'cancelled'].includes(match.status)
  )?.id ?? null

  const handleDeleteArchived = (id: number, name: string) => {
    if (!window.confirm(`Turnier "${name}" endgültig löschen?`)) return
    deleteMutation.mutate(id)
  }

  return (
    <div className="space-y-8">
      <div>
        <h1 className="text-2xl font-bold text-foreground">Turnier-Verwaltung</h1>
        <p className="mt-1 text-sm text-muted">
          Aktive und vergangene Turniere öffnen, pflegen und operative Korrekturen nachziehen.
        </p>
      </div>

      <div className="flex border-b border-border">
        <button
          onClick={() => setActiveTab('verwalten')}
          className={`flex items-center gap-2 border-b-2 px-4 py-3 text-sm font-medium transition-colors ${
            activeTab === 'verwalten'
              ? 'border-primary text-primary'
              : 'border-transparent text-muted hover:text-foreground'
          }`}
        >
          <Settings size={16} />
          Turniere verwalten
          {activeTournament && <span className="ml-1 inline-block h-2 w-2 rounded-full bg-green-400" />}
        </button>
        <button
          onClick={() => setActiveTab('erstellen')}
          className={`flex items-center gap-2 border-b-2 px-4 py-3 text-sm font-medium transition-colors ${
            activeTab === 'erstellen'
              ? 'border-primary text-primary'
              : 'border-transparent text-muted hover:text-foreground'
          }`}
        >
          <Plus size={16} />
          Turnier erstellen
        </button>
      </div>

      {activeTab === 'erstellen' && (
        <div className="space-y-4">
          {activeTournament && (
            <Card className="p-4">
              <p className="text-sm text-warning">
                Solange ein aktives Turnier existiert, blockiert der Server das Anlegen eines weiteren.
              </p>
            </Card>
          )}
          <CreateTournamentForm />
        </div>
      )}

      {activeTab === 'verwalten' && (
        <div className="space-y-6">
          {activeTournament ? (
            <section className="space-y-3">
              <div>
                <h2 className="text-lg font-semibold text-foreground">Aktives Turnier</h2>
                <p className="mt-1 text-sm text-muted">
                  Das aktuell laufende Turnier bleibt mit einem Klick als Arbeitskontext ausgewählt.
                </p>
              </div>

              <Card className="flex flex-col gap-3 p-4 md:flex-row md:items-center md:justify-between">
                <div>
                  <div className="flex items-center gap-2">
                    <span className="font-medium text-foreground">{activeTournament.name}</span>
                    <Badge status={activeTournament.status} />
                  </div>
                  <div className="mt-1 text-sm text-muted">
                    Erstellt am {new Date(activeTournament.created_at).toLocaleDateString('de-DE')}
                  </div>
                </div>

                <Button
                  variant={resolvedSelectedTournamentId === activeTournament.id ? 'primary' : 'secondary'}
                  size="sm"
                  onClick={() => setSelectedTournamentId(activeTournament.id)}
                >
                  {resolvedSelectedTournamentId === activeTournament.id ? 'Geöffnet' : 'Öffnen'}
                </Button>
              </Card>
            </section>
          ) : (
            <Card className="p-5">
              <p className="text-sm text-muted">Kein aktives Turnier vorhanden.</p>
            </Card>
          )}

          <section className="space-y-3">
            <h2 className="flex items-center gap-2 text-lg font-semibold text-foreground">
              <Archive size={18} className="text-muted" />
              Vergangene Turniere ({archived.length})
            </h2>

            {archived.length === 0 ? (
              <Card className="p-5">
                <p className="text-sm text-muted">Keine abgeschlossenen oder archivierten Turniere vorhanden.</p>
              </Card>
            ) : (
              <div className="grid gap-3">
                {archived.map((tournament) => (
                  <Card
                    key={tournament.id}
                    className={`flex flex-col gap-3 p-4 md:flex-row md:items-center md:justify-between ${
                      resolvedSelectedTournamentId === tournament.id ? 'border-primary/50' : ''
                    }`}
                  >
                    <div>
                      <div className="flex items-center gap-2">
                        <span className="font-medium text-foreground">{tournament.name}</span>
                        <Badge status={tournament.status} />
                      </div>
                      <div className="mt-1 text-sm text-muted">
                        Erstellt am {new Date(tournament.created_at).toLocaleDateString('de-DE')}
                      </div>
                    </div>

                    <div className="flex flex-wrap gap-2">
                      <Button
                        variant={resolvedSelectedTournamentId === tournament.id ? 'primary' : 'secondary'}
                        size="sm"
                        onClick={() => setSelectedTournamentId(tournament.id)}
                      >
                        {resolvedSelectedTournamentId === tournament.id ? 'Geöffnet' : 'Öffnen'}
                      </Button>

                      <Button
                        variant="danger"
                        size="sm"
                        disabled={deleteMutation.isPending}
                        onClick={() => handleDeleteArchived(tournament.id, tournament.name)}
                      >
                        <Trash2 size={14} />
                        Löschen
                      </Button>
                    </div>
                  </Card>
                ))}
              </div>
            )}
          </section>

          {isLoadingDetail && resolvedSelectedTournamentId ? (
            <LoadingSpinner />
          ) : selectedDetail ? (
            <>
              <TournamentManager
                key={`${selectedDetail.id}-${selectedDetail.updated_at}`}
                tournament={selectedDetail}
                teamCount={teamCount}
                playerCount={playerCount}
                matchCount={matchCount}
              />

              {canManageParticipants && (
                <ParticipantManager
                  tournamentId={selectedDetail.id}
                  tournamentStatus={selectedDetail.status}
                  teamSize={selectedDetail.team_size}
                  teams={selectedDetail.teams}
                  signups={selectedDetail.signups}
                />
              )}

              {selectedDetail.status === 'checkin' && (
                <CheckinManager
                  tournamentId={selectedDetail.id}
                  teamSize={selectedDetail.team_size}
                  teams={selectedDetail.teams}
                  signups={selectedDetail.signups}
                />
              )}

              {showGroupMatchAdmin && (
                <section className="space-y-3">
                  <div>
                    <h2 className="text-lg font-semibold text-foreground">Gruppenspiel-Ergebnisse</h2>
                    <p className="mt-1 text-sm text-muted">
                      Gruppenspiele für das ausgewählte Turnier prüfen und fehlende Ergebnisse nachtragen.
                    </p>
                  </div>
                  <GroupMatchAdminPanel
                    tournamentId={selectedDetail.id}
                    groups={selectedDetail.groups}
                    teams={selectedDetail.teams}
                    onRefresh={() => void refetchSelectedDetail()}
                  />
                </section>
              )}

              {showBracketAdmin && (
                <section className="space-y-3">
                  <div>
                    <h2 className="text-lg font-semibold text-foreground">Bracket-Matches</h2>
                    <p className="mt-1 text-sm text-muted">
                      Lobbys steuern, Ergebnisse abrufen und Match-Events ohne Hexenwerk live setzen.
                    </p>
                  </div>

                  <VoiceChannelPanel
                    tournamentId={selectedDetail.id}
                    currentMatchId={currentBracketMatchId}
                  />

                  <div className="flex border-b border-border">
                    <button
                      onClick={() => setBracketAdminTab('matches')}
                      className={`border-b-2 px-4 py-3 text-sm font-medium transition-colors ${
                        bracketAdminTab === 'matches'
                          ? 'border-primary text-primary'
                          : 'border-transparent text-muted hover:text-foreground'
                      }`}
                    >
                      Match-Steuerung
                    </button>
                    <button
                      onClick={() => setBracketAdminTab('events')}
                      className={`border-b-2 px-4 py-3 text-sm font-medium transition-colors ${
                        bracketAdminTab === 'events'
                          ? 'border-primary text-primary'
                          : 'border-transparent text-muted hover:text-foreground'
                      }`}
                    >
                      Match-Events
                    </button>
                  </div>

                  {bracketAdminTab === 'matches' ? (
                    <MatchAdminPanel
                      tournamentId={selectedDetail.id}
                      matches={selectedDetail.bracket_matches}
                      teams={selectedDetail.teams}
                      onRefresh={() => void refetchSelectedDetail()}
                      allowManualOverride={allowManualOverride}
                    />
                  ) : (
                    <MatchEventPanel
                      tournamentId={selectedDetail.id}
                      matches={selectedDetail.bracket_matches}
                      teams={selectedDetail.teams}
                      onRefresh={() => void refetchSelectedDetail()}
                    />
                  )}
                </section>
              )}
            </>
          ) : (
            <Card className="p-8 text-center">
              <Settings size={32} className="mx-auto mb-3 text-muted" />
              <p className="text-muted">
                {selectedTournament ? `Turnier "${selectedTournament.name}" konnte nicht geladen werden.` : 'Kein Turnier ausgewählt.'}
              </p>
              <p className="mt-1 text-sm text-muted">
                Wähle ein Turnier aus der Liste oder lege ein neues Turnier an.
              </p>
            </Card>
          )}
        </div>
      )}
    </div>
  )
}
