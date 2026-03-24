import { useState } from 'react'
import { useTournaments, useTournament } from '@/hooks/useTournament'
import Card from '@/components/ui/Card'
import Badge from '@/components/ui/Badge'
import LoadingSpinner from '@/components/ui/LoadingSpinner'
import CreateTournamentForm from '@/components/admin/CreateTournamentForm'
import TournamentManager from '@/components/admin/TournamentManager'
import { Plus, Settings, Archive } from 'lucide-react'

type AdminTab = 'erstellen' | 'verwalten'

export default function Admin() {
  const { data: tournaments, isLoading } = useTournaments()
  const [activeTab, setActiveTab] = useState<AdminTab>('verwalten')

  const activeTournament = tournaments?.find(t =>
    ['draft', 'registration', 'group_phase', 'bracket'].includes(t.status)
  )
  const archived = tournaments?.filter(t =>
    ['completed', 'archived'].includes(t.status)
  ) ?? []

  // Fetch detail data for the active tournament
  const { data: activeDetail } = useTournament(activeTournament?.id ?? 0)

  if (isLoading) return <LoadingSpinner />

  const teamCount = activeDetail?.teams.length ?? 0
  const playerCount = activeDetail?.teams.reduce((sum, t) => sum + t.members.length, 0) ?? 0
  const matchCount = activeDetail?.bracket_matches.length ?? 0

  return (
    <div className="space-y-8">
      {/* Header */}
      <div>
        <h1 className="text-2xl font-bold text-foreground">Turnier-Verwaltung</h1>
        <p className="text-muted text-sm mt-1">Turniere erstellen, verwalten und abschliessen</p>
      </div>

      {/* Tabs */}
      <div className="flex border-b border-border">
        <button
          onClick={() => setActiveTab('erstellen')}
          className={`flex items-center gap-2 px-4 py-3 text-sm font-medium border-b-2 transition-colors ${
            activeTab === 'erstellen'
              ? 'border-primary text-primary'
              : 'border-transparent text-muted hover:text-foreground'
          }`}
        >
          <Plus size={16} />
          Turnier erstellen
        </button>
        <button
          onClick={() => setActiveTab('verwalten')}
          className={`flex items-center gap-2 px-4 py-3 text-sm font-medium border-b-2 transition-colors ${
            activeTab === 'verwalten'
              ? 'border-primary text-primary'
              : 'border-transparent text-muted hover:text-foreground'
          }`}
        >
          <Settings size={16} />
          Aktiv verwalten
          {activeTournament && (
            <span className="ml-1 w-2 h-2 rounded-full bg-green-400 inline-block" />
          )}
        </button>
      </div>

      {/* Tab Content */}
      {activeTab === 'erstellen' && <CreateTournamentForm />}

      {activeTab === 'verwalten' && (
        <div className="space-y-6">
          {activeTournament ? (
            <TournamentManager
              tournament={activeTournament}
              teamCount={teamCount}
              playerCount={playerCount}
              matchCount={matchCount}
            />
          ) : (
            <Card className="p-8 text-center">
              <Settings size={32} className="mx-auto text-muted mb-3" />
              <p className="text-muted">Kein aktives Turnier vorhanden.</p>
              <p className="text-muted text-sm mt-1">
                Erstelle ein neues Turnier im Tab &quot;Turnier erstellen&quot;.
              </p>
            </Card>
          )}
        </div>
      )}

      {/* Archiv */}
      {archived.length > 0 && (
        <section>
          <h2 className="text-lg font-semibold text-foreground mb-3 flex items-center gap-2">
            <Archive size={18} className="text-muted" />
            Archiv ({archived.length})
          </h2>
          <div className="grid gap-2">
            {archived.map(t => (
              <Card key={t.id} className="p-3 flex items-center justify-between">
                <div>
                  <span className="font-medium text-foreground">{t.name}</span>
                  <span className="ml-3 text-sm text-muted">
                    {new Date(t.created_at).toLocaleDateString('de-DE')}
                  </span>
                </div>
                <Badge status={t.status} />
              </Card>
            ))}
          </div>
        </section>
      )}
    </div>
  )
}
