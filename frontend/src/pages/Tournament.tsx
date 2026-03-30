import { useState, useEffect } from 'react'
import type { FormEvent } from 'react'
import { useParams } from 'react-router-dom'
import {
  useTournament, useCreateTeam, useJoinTeam, useSignupSolo,
  useWithdrawSolo, useKickMember, useInviteSoloPlayer, useLeaveTeam,
} from '@/hooks/useTournament'
import { useAuth } from '@/hooks/useAuth'
import Card from '@/components/ui/Card'
import Badge from '@/components/ui/Badge'
import Button from '@/components/ui/Button'
import LoadingSpinner from '@/components/ui/LoadingSpinner'
import GroupStandings from '@/components/groups/GroupStandings'
import GroupMatchList from '@/components/groups/GroupMatchList'
import BracketView from '@/components/bracket/BracketView'
import { Trophy, Users, LayoutGrid, GitBranch, Plus, UserPlus, AlertCircle, Shield, X, Info, ChevronDown, ChevronUp } from 'lucide-react'
import type { Team } from '@/types/tournament'

type Tab = 'übersicht' | 'gruppen' | 'bracket' | 'teams'

const TABS: { key: Tab; label: string; icon: typeof Trophy }[] = [
  { key: 'übersicht', label: 'Übersicht', icon: Trophy },
  { key: 'teams', label: 'Teams', icon: Users },
  { key: 'gruppen', label: 'Gruppen', icon: LayoutGrid },
  { key: 'bracket', label: 'Bracket', icon: GitBranch },
]

export default function Tournament() {
  const { id } = useParams<{ id: string }>()
  const tournamentId = Number(id)
  const { data: tournament, isLoading } = useTournament(tournamentId)
  const { user, isLoggedIn } = useAuth()
  const [activeTab, setActiveTab] = useState<Tab>('übersicht')

  // Team creation form state
  const [showCreateTeam, setShowCreateTeam] = useState(false)
  const [teamName, setTeamName] = useState('')

  // Success message state
  const [successMsg, setSuccessMsg] = useState<string | null>(null)

  // Solo invite section toggle
  const [showSoloInvite, setShowSoloInvite] = useState(false)

  // Confirmation for joining a team when already in one
  const [pendingJoinTeam, setPendingJoinTeam] = useState<Team | null>(null)

  const createTeamMutation = useCreateTeam()
  const joinTeamMutation = useJoinTeam()
  const signupSoloMutation = useSignupSolo()
  const withdrawSoloMutation = useWithdrawSolo(tournamentId)
  const kickMemberMutation = useKickMember(tournamentId)
  const inviteSoloPlayerMutation = useInviteSoloPlayer(tournamentId)
  const leaveTeamMutation = useLeaveTeam(tournamentId)

  // Auto-clear success message after 3 seconds
  useEffect(() => {
    if (!successMsg) return
    const timer = setTimeout(() => setSuccessMsg(null), 3000)
    return () => clearTimeout(timer)
  }, [successMsg])

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

  // Check if user has a solo signup (signup with no team)
  const userSoloSignup = user
    ? tournament.signups.find(s => s.discord_id === user.discord_id && s.team_id === null)
    : null

  // Open solo signups (no team assigned)
  const openSoloSignups = tournament.signups.filter(s => s.team_id === null)

  const isUserCaptain = userTeam != null && userTeam.captain_discord_id === user?.discord_id
  const teamIsFull = userTeam != null && userTeam.members.length >= tournament.team_size

  const mutationError =
    createTeamMutation.error ||
    joinTeamMutation.error ||
    signupSoloMutation.error ||
    withdrawSoloMutation.error ||
    kickMemberMutation.error ||
    inviteSoloPlayerMutation.error ||
    leaveTeamMutation.error

  const isMutating =
    createTeamMutation.isPending ||
    joinTeamMutation.isPending ||
    signupSoloMutation.isPending ||
    withdrawSoloMutation.isPending ||
    kickMemberMutation.isPending ||
    inviteSoloPlayerMutation.isPending ||
    leaveTeamMutation.isPending

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

  const handleJoinTeam = (team: (typeof tournament.teams)[number]) => {
    if (userTeam) {
      setPendingJoinTeam(team)
    } else {
      joinTeamMutation.mutate({ tournamentId, teamId: team.id })
    }
  }

  const handleSignupSolo = () => {
    signupSoloMutation.mutate(tournamentId, {
      onSuccess: () => {
        setSuccessMsg('Du bist jetzt als Solo-Spieler eingetragen!')
      },
    })
  }

  const handleWithdrawSolo = () => {
    withdrawSoloMutation.mutate()
  }

  const handleKickMember = (teamId: number, discordId: string) => {
    kickMemberMutation.mutate({ teamId, discordId })
  }

  const handleLeaveTeam = (teamId: number) => {
    leaveTeamMutation.mutate({ teamId })
  }

  const handleInviteSolo = (teamId: number, discordId: string) => {
    inviteSoloPlayerMutation.mutate({ teamId, discordId })
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

      {/* Success message banner */}
      {successMsg && (
        <div className="flex items-center gap-2 text-green-400 text-sm bg-green-500/10 border border-green-500/20 rounded-lg p-3">
          <span>{successMsg}</span>
        </div>
      )}

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
              {tab.key === 'gruppen' && (
                <span
                  className="relative group"
                  title="Teams spielen in Gruppen gegeneinander (Round-Robin). Die besten 2 jeder Gruppe kommen weiter ins Bracket."
                >
                  <Info size={13} className="text-muted hover:text-foreground transition-colors" />
                  <span className="pointer-events-none absolute bottom-full left-1/2 -translate-x-1/2 mb-2 w-64 rounded-lg bg-card border border-border px-3 py-2 text-xs text-foreground opacity-0 group-hover:opacity-100 transition-opacity z-10 shadow-lg">
                    Teams spielen in Gruppen gegeneinander (Round-Robin). Die besten 2 jeder Gruppe kommen weiter ins Bracket.
                  </span>
                </span>
              )}
              {tab.key === 'bracket' && (
                <span
                  className="relative group"
                  title="K.O.-Runde: Wer verliert, scheidet aus. Wer gewinnt, kommt eine Runde weiter bis zum Finale."
                >
                  <Info size={13} className="text-muted hover:text-foreground transition-colors" />
                  <span className="pointer-events-none absolute bottom-full left-1/2 -translate-x-1/2 mb-2 w-56 rounded-lg bg-card border border-border px-3 py-2 text-xs text-foreground opacity-0 group-hover:opacity-100 transition-opacity z-10 shadow-lg">
                    K.O.-Runde: Wer verliert, scheidet aus. Wer gewinnt, kommt eine Runde weiter bis zum Finale.
                  </span>
                </span>
              )}
            </button>
          )
        })}
      </div>

      {/* Tab Content */}
      <div>
        {activeTab === 'übersicht' && (
          <Card className="p-6">
            <h2 className="text-lg font-semibold text-foreground mb-4">Turnier-Informationen</h2>
            <div className="grid grid-cols-1 sm:grid-cols-2 gap-4 text-sm">
              {tournament.registration_start && (
                <div>
                  <span className="text-muted">Anmeldung Start:</span>
                  <span className="ml-2 text-foreground">
                    {new Date(tournament.registration_start).toLocaleString('de-DE', {
                      day: '2-digit', month: '2-digit', year: 'numeric', hour: '2-digit', minute: '2-digit',
                    })}
                  </span>
                </div>
              )}
              {tournament.registration_end && (
                <div>
                  <span className="text-muted">Anmeldung Ende:</span>
                  <span className="ml-2 text-foreground">
                    {new Date(tournament.registration_end).toLocaleString('de-DE', {
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
            {/* Registration Actions — only when no team AND no solo signup */}
            {isRegistration && isLoggedIn && !userTeam && !userSoloSignup && (
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

            {/* Solo signup status + opt-out */}
            {isRegistration && isLoggedIn && userSoloSignup && !userTeam && (
              <Card className="p-4">
                {mutationError && (
                  <div className="flex items-center gap-2 text-red-400 text-sm bg-red-500/10 border border-red-500/20 rounded-lg p-3 mb-3">
                    <AlertCircle size={16} />
                    <span>
                      {mutationError instanceof Error ? mutationError.message : 'Ein Fehler ist aufgetreten'}
                    </span>
                  </div>
                )}
                <div className="flex items-center gap-3 flex-wrap">
                  <span className="inline-flex items-center gap-1.5 px-3 py-1 rounded-full text-sm font-medium bg-green-600 text-white">
                    ✓ Solo eingetragen
                  </span>
                  <Button
                    variant="ghost"
                    size="sm"
                    onClick={handleWithdrawSolo}
                    disabled={isMutating}
                    className="text-red-400 border border-red-500/40 hover:bg-red-500/10"
                  >
                    {withdrawSoloMutation.isPending ? 'Wird ausgetragen...' : 'Austragen'}
                  </Button>
                </div>
              </Card>
            )}

            {/* User's current team info */}
            {userTeam && (
              <Card className="p-4 border-primary/30">
                <div className="flex items-center justify-between mb-2">
                  <div className="flex items-center gap-2">
                    <Shield size={16} className="text-primary" />
                    <span className="text-sm font-semibold text-primary">Dein Team</span>
                  </div>
                  {isRegistration && (
                    <Button
                      variant="ghost"
                      size="sm"
                      onClick={() => handleLeaveTeam(userTeam.id)}
                      disabled={isMutating || (isUserCaptain && userTeam.members.length > 1)}
                      title={isUserCaptain && userTeam.members.length > 1 ? 'Übergib zuerst die Captain-Rolle' : undefined}
                      className="text-red-400 border border-red-500/40 hover:bg-red-500/10"
                    >
                      {leaveTeamMutation.isPending ? 'Verlasse...' : 'Team verlassen'}
                    </Button>
                  )}
                </div>
                <h3 className="font-medium text-foreground">{userTeam.name}</h3>
                <div className="mt-2 space-y-1">
                  {userTeam.members.map(m => (
                    <div key={m.discord_id} className="flex items-center gap-2 text-sm">
                      <span className="text-foreground flex-1">{m.discord_name ?? m.discord_id}</span>
                      {m.role === 'captain' && (
                        <span className="text-xs text-primary font-medium">Captain</span>
                      )}
                      {m.rank && (
                        <span className="text-xs text-muted">{m.rank}</span>
                      )}
                      {/* Captain: kick button for non-captain members */}
                      {isUserCaptain && isRegistration && m.discord_id !== user?.discord_id && (
                        <button
                          onClick={() => handleKickMember(userTeam.id, m.discord_id)}
                          disabled={isMutating}
                          title="Mitglied entfernen"
                          className="ml-1 text-red-400 hover:text-red-300 disabled:opacity-50 transition-colors"
                        >
                          <X size={14} />
                        </button>
                      )}
                    </div>
                  ))}
                </div>

                {/* Captain: invite solo players */}
                {isUserCaptain && isRegistration && !teamIsFull && (
                  <div className="mt-4 border-t border-border pt-3">
                    <button
                      className="flex items-center gap-1.5 text-sm font-medium text-foreground hover:text-primary transition-colors"
                      onClick={() => setShowSoloInvite(v => !v)}
                    >
                      <UserPlus size={14} />
                      Solo-Spieler einladen
                      {showSoloInvite ? <ChevronUp size={14} /> : <ChevronDown size={14} />}
                    </button>
                    {showSoloInvite && (
                      <div className="mt-2 space-y-2">
                        {openSoloSignups.length === 0 ? (
                          <p className="text-sm text-muted">Keine offenen Solo-Anmeldungen</p>
                        ) : (
                          openSoloSignups.map(signup => (
                            <div key={signup.discord_id} className="flex items-center justify-between gap-2">
                              <div>
                                <p className="text-sm font-medium text-foreground">{signup.discord_name || signup.discord_id}</p>
                                <p className="text-xs text-muted">{signup.discord_id}</p>
                              </div>
                              <Button
                                variant="secondary"
                                size="sm"
                                onClick={() => handleInviteSolo(userTeam.id, signup.discord_id)}
                                disabled={isMutating}
                              >
                                {inviteSoloPlayerMutation.isPending ? 'Eingeladen...' : 'Einladen'}
                              </Button>
                            </div>
                          ))
                        )}
                      </div>
                    )}
                  </div>
                )}
              </Card>
            )}

            {/* Error shown outside of registration card context */}
            {mutationError && !isRegistration && (
              <div className="flex items-center gap-2 text-red-400 text-sm bg-red-500/10 border border-red-500/20 rounded-lg p-3">
                <AlertCircle size={16} />
                <span>
                  {mutationError instanceof Error ? mutationError.message : 'Ein Fehler ist aufgetreten'}
                </span>
              </div>
            )}

            {/* Open solo signups list */}
            {openSoloSignups.length > 0 && (
              <Card className="p-4">
                <h3 className="text-sm font-semibold text-foreground mb-3">Offene Solo-Anmeldungen</h3>
                <div className="space-y-2">
                  {openSoloSignups.map(signup => (
                    <div key={signup.discord_id} className="text-sm">
                      <p className="font-semibold text-foreground">{signup.discord_name || signup.discord_id}</p>
                      <p className="text-xs text-muted">{signup.discord_id}</p>
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

                    {/* Join Button — visible when no team OR in a different team */}
                    {isRegistration && isLoggedIn && (!userTeam || userTeam.id !== team.id) && team.members.length < tournament.team_size && (
                      <Button
                        variant="secondary"
                        size="sm"
                        onClick={() => handleJoinTeam(team)}
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

      {/* Confirmation Modal — Team wechseln */}
      {pendingJoinTeam && userTeam && (
        <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-50">
          <div className="bg-card border border-border rounded-xl p-6 max-w-sm w-full mx-4 shadow-xl">
            <h3 className="text-base font-semibold text-foreground mb-3">Team wechseln?</h3>
            <p className="text-sm text-muted mb-5">
              Du bist bereits in Team <span className="text-foreground font-medium">"{userTeam.name}"</span>.
              Wenn du <span className="text-foreground font-medium">"{pendingJoinTeam.name}"</span> beitrittst,
              verlässt du dein aktuelles Team automatisch.
            </p>
            <div className="flex gap-2 justify-end">
              <Button
                variant="ghost"
                size="sm"
                onClick={() => setPendingJoinTeam(null)}
                disabled={joinTeamMutation.isPending}
              >
                Abbrechen
              </Button>
              <Button
                variant="primary"
                size="sm"
                onClick={() => {
                  joinTeamMutation.mutate(
                    { tournamentId, teamId: pendingJoinTeam.id },
                    { onSettled: () => setPendingJoinTeam(null) }
                  )
                }}
                disabled={joinTeamMutation.isPending}
              >
                {joinTeamMutation.isPending ? 'Wechsle...' : 'Team wechseln'}
              </Button>
            </div>
          </div>
        </div>
      )}
    </div>
  )
}
