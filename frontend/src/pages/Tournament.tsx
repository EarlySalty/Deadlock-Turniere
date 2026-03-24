import { useState } from 'react'
import type { FormEvent } from 'react'
import { useParams } from 'react-router-dom'
import { useTournament, useCreateTeam, useJoinTeam, useSignupSolo } from '@/hooks/useTournament'
import { useAuth } from '@/hooks/useAuth'
import Card from '@/components/ui/Card'
import Badge from '@/components/ui/Badge'
import Button from '@/components/ui/Button'
import LoadingSpinner from '@/components/ui/LoadingSpinner'
import GroupStandings from '@/components/groups/GroupStandings'
import GroupMatchList from '@/components/groups/GroupMatchList'
import BracketView from '@/components/bracket/BracketView'
import { Trophy, Users, LayoutGrid, GitBranch, Plus, UserPlus, AlertCircle, Shield } from 'lucide-react'

type Tab = 'uebersicht' | 'gruppen' | 'bracket' | 'teams'

const TABS: { key: Tab; label: string; icon: typeof Trophy }[] = [
  { key: 'uebersicht', label: 'Uebersicht', icon: Trophy },
  { key: 'teams', label: 'Teams', icon: Users },
  { key: 'gruppen', label: 'Gruppen', icon: LayoutGrid },
  { key: 'bracket', label: 'Bracket', icon: GitBranch },
]

export default function Tournament() {
  const { id } = useParams<{ id: string }>()
  const tournamentId = Number(id)
  const { data: tournament, isLoading } = useTournament(tournamentId)
  const { user, isLoggedIn } = useAuth()
  const [activeTab, setActiveTab] = useState<Tab>('uebersicht')

  // Team creation form state
  const [showCreateTeam, setShowCreateTeam] = useState(false)
  const [teamName, setTeamName] = useState('')

  const createTeamMutation = useCreateTeam()
  const joinTeamMutation = useJoinTeam()
  const signupSoloMutation = useSignupSolo()

  if (isLoading) return <LoadingSpinner />
  if (!tournament) {
    return (
      <Card className="text-center py-10">
        <p className="text-muted">Turnier nicht gefunden</p>
      </Card>
    )
  }

  const isRegistration = tournament.status === 'registration'

  // Check if user is already in a team
  const userTeam = user
    ? tournament.teams.find(t =>
        t.members.some(m => m.discord_id === user.discord_id)
      )
    : null

  const mutationError =
    createTeamMutation.error || joinTeamMutation.error || signupSoloMutation.error
  const isMutating =
    createTeamMutation.isPending || joinTeamMutation.isPending || signupSoloMutation.isPending

  const handleCreateTeam = (e: FormEvent) => {
    e.preventDefault()
    if (!teamName.trim()) return
    createTeamMutation.mutate(
      { tournamentId, name: teamName.trim() },
      {
        onSuccess: () => {
          setTeamName('')
          setShowCreateTeam(false)
        },
      }
    )
  }

  const handleJoinTeam = (teamId: number) => {
    joinTeamMutation.mutate({ tournamentId, teamId })
  }

  const handleSignupSolo = () => {
    signupSoloMutation.mutate(tournamentId)
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
          <div className="space-y-4">
            {/* Registration Actions */}
            {isRegistration && isLoggedIn && !userTeam && (
              <Card className="p-4">
                <h3 className="text-sm font-semibold text-foreground mb-3">Anmeldung</h3>

                {/* Error */}
                {mutationError && (
                  <div className="flex items-center gap-2 text-red-400 text-sm bg-red-500/10 border border-red-500/20 rounded-lg p-3 mb-3">
                    <AlertCircle size={16} />
                    <span>
                      {mutationError instanceof Error ? mutationError.message : 'Ein Fehler ist aufgetreten'}
                    </span>
                  </div>
                )}

                <div className="flex flex-wrap gap-2">
                  <Button
                    variant="primary"
                    size="sm"
                    onClick={() => setShowCreateTeam(!showCreateTeam)}
                    disabled={isMutating}
                  >
                    <Plus size={14} />
                    Team erstellen
                  </Button>
                  <Button
                    variant="secondary"
                    size="sm"
                    onClick={handleSignupSolo}
                    disabled={isMutating}
                  >
                    <UserPlus size={14} />
                    {signupSoloMutation.isPending ? 'Wird angemeldet...' : 'Solo anmelden'}
                  </Button>
                </div>

                {/* Team-Create Form */}
                {showCreateTeam && (
                  <form onSubmit={handleCreateTeam} className="mt-3 flex gap-2">
                    <input
                      type="text"
                      value={teamName}
                      onChange={(e) => setTeamName(e.target.value)}
                      placeholder="Teamname eingeben..."
                      required
                      className="flex-1 bg-background border border-border rounded-lg px-3 py-2 text-sm text-foreground placeholder:text-muted focus:outline-none focus:ring-2 focus:ring-primary/50"
                    />
                    <Button
                      type="submit"
                      variant="primary"
                      size="sm"
                      disabled={createTeamMutation.isPending || !teamName.trim()}
                    >
                      {createTeamMutation.isPending ? 'Erstellt...' : 'Erstellen'}
                    </Button>
                    <Button
                      type="button"
                      variant="ghost"
                      size="sm"
                      onClick={() => { setShowCreateTeam(false); setTeamName('') }}
                    >
                      Abbrechen
                    </Button>
                  </form>
                )}
              </Card>
            )}

            {/* User's current team info */}
            {userTeam && (
              <Card className="p-4 border-primary/30">
                <div className="flex items-center gap-2 mb-2">
                  <Shield size={16} className="text-primary" />
                  <span className="text-sm font-semibold text-primary">Dein Team</span>
                </div>
                <h3 className="font-medium text-foreground">{userTeam.name}</h3>
                <div className="mt-2 space-y-1">
                  {userTeam.members.map(m => (
                    <div key={m.discord_id} className="flex items-center gap-2 text-sm">
                      <span className="text-foreground">{m.discord_name ?? m.discord_id}</span>
                      {m.role === 'captain' && (
                        <span className="text-xs text-primary font-medium">Captain</span>
                      )}
                      {m.rank && (
                        <span className="text-xs text-muted">{m.rank}</span>
                      )}
                    </div>
                  ))}
                </div>
              </Card>
            )}

            {/* Team List */}
            <div className="grid gap-3">
              {tournament.teams.length > 0 ? tournament.teams.map(team => (
                <Card key={team.id} className="p-4">
                  <div className="flex items-center justify-between">
                    <div className="flex-1">
                      <div className="flex items-center gap-2">
                        <h3 className="font-medium text-foreground">{team.name}</h3>
                        <span className="text-sm text-muted">
                          {team.members.length}/{tournament.team_size} Mitglieder
                        </span>
                      </div>
                      {/* Members */}
                      <div className="mt-2 flex flex-wrap gap-x-4 gap-y-1">
                        {team.members.map(m => (
                          <span key={m.discord_id} className="text-xs text-muted">
                            {m.discord_name ?? m.discord_id}
                            {m.role === 'captain' && (
                              <span className="ml-1 text-primary">(C)</span>
                            )}
                            {m.rank && (
                              <span className="ml-1 opacity-60">[{m.rank}]</span>
                            )}
                          </span>
                        ))}
                      </div>
                    </div>

                    {/* Join Button */}
                    {isRegistration && isLoggedIn && !userTeam && team.members.length < tournament.team_size && (
                      <Button
                        variant="secondary"
                        size="sm"
                        onClick={() => handleJoinTeam(team.id)}
                        disabled={isMutating}
                      >
                        <UserPlus size={14} />
                        Beitreten
                      </Button>
                    )}
                  </div>
                </Card>
              )) : (
                <Card className="text-center py-8">
                  <Users size={32} className="mx-auto text-muted mb-3" />
                  <p className="text-muted">Noch keine Teams angemeldet</p>
                </Card>
              )}
            </div>
          </div>
        )}

        {activeTab === 'gruppen' && (
          <div className="space-y-6">
            <GroupStandings groups={tournament.groups} />
            <GroupMatchList groups={tournament.groups} teams={tournament.teams} />
          </div>
        )}

        {activeTab === 'bracket' && (
          <BracketView matches={tournament.bracket_matches} teams={tournament.teams} />
        )}
      </div>
    </div>
  )
}
