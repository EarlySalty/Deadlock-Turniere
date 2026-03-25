import { useState } from 'react'
import { useAdminTournament, useAdminTournaments, useDeleteTournament } from '@/hooks/useTournament'
import Card from '@/components/ui/Card'
import Badge from '@/components/ui/Badge'
import Button from '@/components/ui/Button'
import LoadingSpinner from '@/components/ui/LoadingSpinner'
import CreateTournamentForm from '@/components/admin/CreateTournamentForm'
import TournamentManager from '@/components/admin/TournamentManager'
import ParticipantManager from '@/components/admin/ParticipantManager'
import MatchAdminPanel from '@/components/admin/MatchAdminPanel'
import { Archive, Plus, Settings, Trash2 } from 'lucide-react'

type AdminTab = 'erstellen' | 'verwalten'

export default function Admin() {
  const { data: tournaments, isLoading } = useAdminTournaments()
  const [activeTab, setActiveTab] = useState<AdminTab>('verwalten')
  const deleteMutation = useDeleteTournament()

  const activeTournament = tournaments?.find((tournament) =>
    ['draft', 'registration', 'group_phase', 'bracket'].includes(tournament.status)
  )
  const archived = tournaments?.filter((tournament) =>
    ['completed', 'archived'].includes(tournament.status)
  ) ?? []

  const { data: activeDetail, refetch: refetchActiveDetail } = useAdminTournament(activeTournament?.id ?? 0)

  if (isLoading) return <LoadingSpinner />

  const teamCount = activeDetail?.teams.length ?? 0
  const playerCount = activeDetail?.teams.reduce((sum, team) => sum + team.members.length, 0) ?? 0
  const matchCount =
    (activeDetail?.bracket_matches.length ?? 0) +
    (activeDetail?.groups.reduce((sum, group) => sum + group.matches.length, 0) ?? 0)

  const handleDeleteArchived = (id: number, name: string) => {
    if (!window.confirm(`Turnier "${name}" endgültig löschen?`)) return
    deleteMutation.mutate(id)
  }

  return (
    <div className="space-y-8">
      <div>
        <h1 className="text-2xl font-bold text-foreground">Turnier-Verwaltung</h1>
        <p className="mt-1 text-sm text-muted">
          Ein aktives Turnier steuern, Teilnehmer verwalten und abgeschlossene Events bereinigen.
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
          Aktives Turnier
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
          {activeTournament && activeDetail ? (
            <>
              <TournamentManager
                key={`${activeDetail.id}-${activeDetail.updated_at}`}
                tournament={activeDetail}
                teamCount={teamCount}
                playerCount={playerCount}
                matchCount={matchCount}
              />

              <ParticipantManager
                tournamentId={activeDetail.id}
                teamSize={activeDetail.team_size}
                teams={activeDetail.teams}
                signups={activeDetail.signups}
              />

              {activeTournament.status === 'bracket' && (
                <section className="space-y-3">
                  <div>
                    <h2 className="text-lg font-semibold text-foreground">Steam Match-Steuerung</h2>
                    <p className="mt-1 text-sm text-muted">
                      Lobbys erstellen, Match-Start auslösen und Ergebnisse automatisch ziehen.
                    </p>
                  </div>
                  <MatchAdminPanel
                    tournamentId={activeDetail.id}
                    matches={activeDetail.bracket_matches}
                    teams={activeDetail.teams}
                    onRefresh={() => void refetchActiveDetail()}
                  />
                </section>
              )}
            </>
          ) : (
            <Card className="p-8 text-center">
              <Settings size={32} className="mx-auto mb-3 text-muted" />
              <p className="text-muted">Kein aktives Turnier vorhanden.</p>
              <p className="mt-1 text-sm text-muted">
                Lege ein neues Turnier an oder arbeite nur noch im Archiv.
              </p>
            </Card>
          )}
        </div>
      )}

      <section className="space-y-3">
        <h2 className="flex items-center gap-2 text-lg font-semibold text-foreground">
          <Archive size={18} className="text-muted" />
          Archiv ({archived.length})
        </h2>

        {archived.length === 0 ? (
          <Card className="p-5">
            <p className="text-sm text-muted">Keine abgeschlossenen oder archivierten Turniere vorhanden.</p>
          </Card>
        ) : (
          <div className="grid gap-3">
            {archived.map((tournament) => (
              <Card key={tournament.id} className="flex flex-col gap-3 p-4 md:flex-row md:items-center md:justify-between">
                <div>
                  <div className="flex items-center gap-2">
                    <span className="font-medium text-foreground">{tournament.name}</span>
                    <Badge status={tournament.status} />
                  </div>
                  <div className="mt-1 text-sm text-muted">
                    Erstellt am {new Date(tournament.created_at).toLocaleDateString('de-DE')}
                  </div>
                </div>

                <Button
                  variant="danger"
                  size="sm"
                  disabled={deleteMutation.isPending}
                  onClick={() => handleDeleteArchived(tournament.id, tournament.name)}
                >
                  <Trash2 size={14} />
                  Löschen
                </Button>
              </Card>
            ))}
          </div>
        )}
      </section>
    </div>
  )
}
