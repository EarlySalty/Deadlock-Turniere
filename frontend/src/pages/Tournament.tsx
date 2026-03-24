import { useState } from 'react'
import { useParams } from 'react-router-dom'
import { useTournament } from '@/hooks/useTournament'
import Card from '@/components/ui/Card'
import Badge from '@/components/ui/Badge'
import LoadingSpinner from '@/components/ui/LoadingSpinner'
import { Trophy, Users, LayoutGrid, GitBranch } from 'lucide-react'

type Tab = 'uebersicht' | 'gruppen' | 'bracket' | 'teams'

const TABS: { key: Tab; label: string; icon: typeof Trophy }[] = [
  { key: 'uebersicht', label: 'Übersicht', icon: Trophy },
  { key: 'teams', label: 'Teams', icon: Users },
  { key: 'gruppen', label: 'Gruppen', icon: LayoutGrid },
  { key: 'bracket', label: 'Bracket', icon: GitBranch },
]

export default function Tournament() {
  const { id } = useParams<{ id: string }>()
  const { data: tournament, isLoading } = useTournament(Number(id))
  const [activeTab, setActiveTab] = useState<Tab>('uebersicht')

  if (isLoading) return <LoadingSpinner />
  if (!tournament) {
    return (
      <Card className="text-center py-10">
        <p className="text-muted">Turnier nicht gefunden</p>
      </Card>
    )
  }

  return (
    <div className="space-y-6">
      {/* Header */}
      <div>
        <div className="flex items-center gap-3 mb-2">
          <h1 className="text-2xl font-bold text-foreground">{tournament.name}</h1>
          <Badge status={tournament.status} />
        </div>
        {tournament.description && (
          <p className="text-muted">{tournament.description}</p>
        )}
        <div className="flex items-center gap-4 mt-3 text-sm text-muted">
          <span>{tournament.team_size}er Teams</span>
          <span>{tournament.teams.length} Teams</span>
          <span>{tournament.bracket_format === 'single_elimination' ? 'Single Elimination' : 'Double Elimination'}</span>
        </div>
      </div>

      {/* Tabs */}
      <div className="flex border-b border-border">
        {TABS.map(tab => {
          const Icon = tab.icon
          return (
            <button
              key={tab.key}
              onClick={() => setActiveTab(tab.key)}
              className={`flex items-center gap-2 px-4 py-3 text-sm font-medium border-b-2 transition-colors ${
                activeTab === tab.key
                  ? 'border-primary text-primary'
                  : 'border-transparent text-muted hover:text-foreground'
              }`}
            >
              <Icon size={16} />
              {tab.label}
            </button>
          )
        })}
      </div>

      {/* Tab Content */}
      <div>
        {activeTab === 'uebersicht' && (
          <Card className="p-6">
            <h2 className="text-lg font-semibold text-foreground mb-4">Turnier-Informationen</h2>
            <div className="grid grid-cols-1 sm:grid-cols-2 gap-4 text-sm">
              {tournament.registration_start && (
                <div>
                  <span className="text-muted">Anmeldung Start:</span>
                  <span className="ml-2 text-foreground">
                    {new Date(tournament.registration_start).toLocaleDateString('de-DE', {
                      day: '2-digit', month: '2-digit', year: 'numeric', hour: '2-digit', minute: '2-digit',
                    })}
                  </span>
                </div>
              )}
              {tournament.registration_end && (
                <div>
                  <span className="text-muted">Anmeldung Ende:</span>
                  <span className="ml-2 text-foreground">
                    {new Date(tournament.registration_end).toLocaleDateString('de-DE', {
                      day: '2-digit', month: '2-digit', year: 'numeric', hour: '2-digit', minute: '2-digit',
                    })}
                  </span>
                </div>
              )}
            </div>
          </Card>
        )}

        {activeTab === 'teams' && (
          <div className="grid gap-3">
            {tournament.teams.length > 0 ? tournament.teams.map(team => (
              <Card key={team.id} className="p-4">
                <div className="flex items-center justify-between">
                  <div>
                    <h3 className="font-medium text-foreground">{team.name}</h3>
                    <span className="text-sm text-muted">{team.members.length} Mitglieder</span>
                  </div>
                </div>
              </Card>
            )) : (
              <Card className="text-center py-8">
                <p className="text-muted">Noch keine Teams angemeldet</p>
              </Card>
            )}
          </div>
        )}

        {activeTab === 'gruppen' && (
          <Card className="p-6 text-center">
            <LayoutGrid size={32} className="mx-auto text-muted mb-3" />
            <p className="text-muted">Gruppenphase wird hier angezeigt</p>
          </Card>
        )}

        {activeTab === 'bracket' && (
          <Card className="p-6 text-center">
            <GitBranch size={32} className="mx-auto text-muted mb-3" />
            <p className="text-muted">Bracket-Ansicht wird hier angezeigt</p>
          </Card>
        )}
      </div>
    </div>
  )
}
