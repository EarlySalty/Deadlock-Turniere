import { useState, useEffect } from 'react'
import type { FormEvent } from 'react'
import { useParams } from 'react-router-dom'
import { motion } from 'framer-motion'
import {
  useTournament, useMyTournamentStatus, useCreateTeam, useJoinTeam, useSignupSolo,
  useWithdrawSolo, useLeaveTeam, useCheckin, useCheckinStatus,
  useMyInvitations, useAcceptInvitation, useRejectInvitation, useApplyToTeam,
  useConsent,
} from '@/hooks/useTournament'
import { useAuth } from '@/hooks/useAuth'
import Card from '@/components/ui/Card'
import Badge from '@/components/ui/Badge'
import Button from '@/components/ui/Button'
import LoadingSpinner from '@/components/ui/LoadingSpinner'
import ConsentModal from '@/components/ConsentModal'
import GroupStandings from '@/components/groups/GroupStandings'
import GroupMatchList from '@/components/groups/GroupMatchList'
import BracketView from '@/components/bracket/BracketView'
import MiniGroupPanel from '@/components/bracket/MiniGroupPanel'
import MyMatchReport from '@/components/MyMatchReport'
import LoginButton from '@/components/auth/LoginButton'
import {
  Trophy, Users, LayoutGrid, GitBranch, Shield, X, Info, Book,
  ScrollText, CheckCircle2, ClipboardCheck, Mail, BarChart2, User,
} from 'lucide-react'
import type { TeamPublic, BracketMatch, GroupMatch, TournamentGameMode, TournamentDetailPublic } from '@/types/tournament'

const GAME_MODE_LABEL: Record<TournamentGameMode, string> = {
  standard: 'Standard',
  mirror: 'Mirror Match',
  all_same: 'All Same Hero',
  random_heroes: 'Random Heroes',
  single_lane: 'Single Lane Battle',
}
import { ApiError } from '@/api/client'
import ReactMarkdown from 'react-markdown'

type Tab = 'übersicht' | 'kodex' | 'gruppen' | 'bracket' | 'teams' | 'ergebnisse' | 'rangliste'

const ALL_TABS: { key: Tab; label: string; icon: typeof Trophy }[] = [
  { key: 'übersicht', label: 'Übersicht', icon: Trophy },
  { key: 'kodex', label: 'Regelwerk', icon: Book },
  { key: 'teams', label: 'Teams', icon: Users },
  { key: 'gruppen', label: 'Gruppen', icon: LayoutGrid },
  { key: 'bracket', label: 'Bracket', icon: GitBranch },
  { key: 'rangliste', label: 'Rangliste', icon: BarChart2 },
  { key: 'ergebnisse', label: 'Ergebnisse', icon: ScrollText },
]

interface ResultEntry {
  id: string
  title: string
  winnerName: string
  loserName: string
  playedAt: string | null
  sortTime: number
}

function safePublicName(name: string | null | undefined): string {
  const trimmed = name?.trim()
  return trimmed ? trimmed : 'Unbekannt'
}

function getTeamName(teamId: number | null, teams: TeamPublic[]): string {
  if (teamId === null) return 'Freilos'
  return teams.find((team) => team.id === teamId)?.name ?? `Team #${teamId}`
}

interface BracketRankEntry {
  position: number
  teamId: number
  teamName: string
  label: string
}

function deriveBracketPlacements(matches: BracketMatch[], teams: TeamPublic[]): BracketRankEntry[] {
  const completed = matches.filter(
    (m) => m.bracket_type === 'winners'
      && (m.status === 'completed' || m.status === 'forfeit')
      && m.winner_id !== null,
  )
  if (completed.length === 0) return []

  const maxRound = Math.max(...completed.map((m) => m.round))
  const entries: BracketRankEntry[] = []

  for (const match of completed) {
    const distance = maxRound - match.round
    const winnerId = match.winner_id!
    const loserId = match.team1_id === winnerId ? match.team2_id : match.team1_id

    if (distance === 0) {
      entries.push({ position: 1, teamId: winnerId, teamName: getTeamName(winnerId, teams), label: '1. Platz' })
      if (loserId !== null) {
        entries.push({ position: 2, teamId: loserId, teamName: getTeamName(loserId, teams), label: '2. Platz' })
      }
    } else {
      const startPos = Math.pow(2, distance) + 1
      const label = distance === 1 ? '3./4. Platz' : distance === 2 ? '5.–8. Platz' : '9.+ Platz'
      if (loserId !== null) {
        entries.push({ position: startPos, teamId: loserId, teamName: getTeamName(loserId, teams), label })
      }
    }
  }

  const seen = new Map<number, BracketRankEntry>()
  for (const entry of entries) {
    const existing = seen.get(entry.teamId)
    if (!existing || entry.position < existing.position) {
      seen.set(entry.teamId, entry)
    }
  }

  return Array.from(seen.values()).sort((a, b) => a.position - b.position)
}

function getSortTime(value: string | null, fallback: number): number {
  if (!value) return Number.MAX_SAFE_INTEGER - fallback
  const parsed = new Date(value).getTime()
  return Number.isNaN(parsed) ? Number.MAX_SAFE_INTEGER - fallback : parsed
}

function isCompletedMatch(status: string): boolean {
  return status === 'completed' || status === 'forfeit'
}

function getBracketRoundLabel(round: number, maxRound: number): string {
  if (maxRound <= 1) return 'Finale'
  if (round === maxRound) return 'Finale'
  if (round === maxRound - 1) return 'Halbfinale'
  if (round === maxRound - 2) return 'Viertelfinale'
  return `Runde ${round}`
}

function buildGroupResultEntries(
  groups: { name: string; matches: GroupMatch[] }[],
  teams: TeamPublic[],
): ResultEntry[] {
  return groups.flatMap((group) => {
    const completedMatches = group.matches
      .filter((match) => isCompletedMatch(match.status) && match.winner_id !== null)
      .sort((left, right) => getSortTime(left.played_at, left.id) - getSortTime(right.played_at, right.id))
    return completedMatches.map((match, index) => {
      const winnerName = getTeamName(match.winner_id, teams)
      const loserId = match.winner_id === match.team1_id ? match.team2_id : match.team1_id
      return {
        id: `group-${match.id}`,
        title: `${group.name} • Runde ${index + 1}`,
        winnerName,
        loserName: getTeamName(loserId, teams),
        playedAt: match.played_at,
        sortTime: getSortTime(match.played_at, match.id),
      }
    })
  })
}

function buildBracketResultEntries(matches: BracketMatch[], teams: TeamPublic[]): ResultEntry[] {
  const maxRound = matches.reduce((highest, match) => Math.max(highest, match.round), 0)
  return matches
    .filter((match) => (
      isCompletedMatch(match.status)
      && match.winner_id !== null
      && match.team1_id !== null
      && match.team2_id !== null
    ))
    .map((match) => {
      const winnerName = getTeamName(match.winner_id, teams)
      const loserId = match.winner_id === match.team1_id ? match.team2_id : match.team1_id
      return {
        id: `bracket-${match.id}`,
        title: `Bracket • ${getBracketRoundLabel(match.round, maxRound)}`,
        winnerName,
        loserName: getTeamName(loserId, teams),
        playedAt: match.played_at,
        sortTime: getSortTime(match.played_at, match.id),
      }
    })
}

function recruitingLabel(status: string) {
  if (status === 'application') return { text: 'Bewerbung', color: 'text-amber-400 bg-amber-500/15 border-amber-500/30' }
  if (status === 'closed') return { text: 'Geschlossen', color: 'text-muted bg-border/30 border-border' }
  return { text: 'Offen', color: 'text-green-400 bg-green-500/15 border-green-500/30' }
}

export default function Tournament() {
  const { id } = useParams<{ id: string }>()
  const tournamentId = Number(id)
  const { data: tournament, isLoading } = useTournament(tournamentId)
  const { data: checkinStatus } = useCheckinStatus(tournamentId)
  const { isLoggedIn } = useAuth()
  const [activeTab, setActiveTab] = useState<Tab>('übersicht')
  const [showCreateTeam, setShowCreateTeam] = useState(false)
  const [teamName, setTeamName] = useState('')
  const [successMsg, setSuccessMsg] = useState<string | null>(null)
  const [pendingAction, setPendingAction] = useState<(() => void) | null>(null)
  const [showConsentModal, setShowConsentModal] = useState(false)
  const [pendingJoinTeam, setPendingJoinTeam] = useState<TeamPublic | null>(null)

  const { data: myStatus } = useMyTournamentStatus(tournamentId, isLoggedIn)
  const { data: myInvitations } = useMyInvitations(tournamentId, isLoggedIn && !myStatus?.team_id)
  const { data: consent } = useConsent()

  const createTeamMutation = useCreateTeam()
  const joinTeamMutation = useJoinTeam()
  const signupSoloMutation = useSignupSolo()
  const withdrawSoloMutation = useWithdrawSolo(tournamentId)
  const leaveTeamMutation = useLeaveTeam(tournamentId)
  const checkinMutation = useCheckin(tournamentId)
  const acceptInviteMutation = useAcceptInvitation(tournamentId)
  const rejectInviteMutation = useRejectInvitation(tournamentId)
  const applyMutation = useApplyToTeam(tournamentId)

  useEffect(() => {
    if (!successMsg) return
    const timer = setTimeout(() => setSuccessMsg(null), 3000)
    return () => clearTimeout(timer)
  }, [successMsg])

  const showResultsTab = tournament
    ? ['group_phase', 'bracket', 'completed', 'archived'].includes(tournament.status)
    : false
  const availableTabs = ALL_TABS.filter(
    (tab) => showResultsTab || (tab.key !== 'ergebnisse' && tab.key !== 'rangliste')
  )

  // Reset unavailable selection during render, before displaying a stale tab.
  if (tournament && !availableTabs.some((tab) => tab.key === activeTab)) {
    setActiveTab('übersicht')
  }

  if (isLoading) return <LoadingSpinner />
  if (!tournament) {
    return (
      <Card className="text-center py-20 opacity-60">
        <p className="text-muted italic">Die Arena scheint leer zu sein...</p>
      </Card>
    )
  }

  const isRegistration = tournament.status === 'registration'
  const isSignupOpen = ['registration', 'checkin'].includes(tournament.status)
  const isCheckinPhase = tournament.status === 'checkin'

  // User state from status endpoint (no discord_id scanning needed)
  const userTeam = myStatus?.team_id != null
    ? tournament.teams.find((t) => t.id === myStatus.team_id) ?? null
    : null
  const userSignupId = myStatus?.signup_id ?? null
  const userHasSoloSignup = userSignupId !== null && !myStatus?.team_id
  const isUserCaptain = myStatus?.is_captain ?? false
  const isUserRegistered = Boolean(userTeam || userHasSoloSignup)
  const hasCheckedIn = myStatus?.is_checked_in ?? false

  const resultEntries = tournament
    ? [
        ...buildGroupResultEntries(tournament.groups, tournament.teams),
        ...buildBracketResultEntries(tournament.bracket_matches, tournament.teams),
      ].sort((left, right) => left.sortTime - right.sortTime || left.id.localeCompare(right.id))
    : []

  const isMutating =
    createTeamMutation.isPending ||
    joinTeamMutation.isPending ||
    signupSoloMutation.isPending ||
    withdrawSoloMutation.isPending ||
    leaveTeamMutation.isPending ||
    checkinMutation.isPending ||
    acceptInviteMutation.isPending ||
    rejectInviteMutation.isPending ||
    applyMutation.isPending

  // Consent-aware action wrapper
  const withConsent = (action: () => void) => {
    if (!isLoggedIn) return
    if (consent?.has_consent) {
      action()
    } else {
      setPendingAction(() => action)
      setShowConsentModal(true)
    }
  }

  const handleConsentError = (error: unknown) => {
    if (error instanceof ApiError && error.message === 'CONSENT_REQUIRED') {
      setPendingAction(null)
      setShowConsentModal(true)
    }
  }

  const handleCreateTeam = (e: FormEvent) => {
    e.preventDefault()
    if (!teamName.trim()) return
    withConsent(() => {
      createTeamMutation.mutate(
        { tournamentId, name: teamName.trim() },
        {
          onSuccess: () => { setTeamName(''); setShowCreateTeam(false) },
          onError: handleConsentError,
        }
      )
    })
  }

  const handleJoinTeam = (team: TeamPublic) => {
    withConsent(() => {
      if (userTeam) {
        setPendingJoinTeam(team)
      } else {
        joinTeamMutation.mutate({ tournamentId, teamId: team.id }, { onError: handleConsentError })
      }
    })
  }

  const handleSignupSolo = () => {
    withConsent(() => {
      signupSoloMutation.mutate(tournamentId, {
        onSuccess: () => setSuccessMsg('Du bist jetzt als Solo-Spieler eingetragen!'),
        onError: handleConsentError,
      })
    })
  }

  const handleWithdrawSolo = () => withdrawSoloMutation.mutate()
  const handleLeaveTeam = (teamId: number) => leaveTeamMutation.mutate({ teamId })

  const handleCheckin = () => {
    checkinMutation.mutate(undefined, {
      onSuccess: (result) => {
        setSuccessMsg(
          result.already_checked_in
            ? 'Du warst bereits eingecheckt.'
            : 'Check-in bestätigt. Du bist für den Turnierstart markiert.'
        )
      },
    })
  }

  const handleApply = (teamId: number) => {
    withConsent(() => {
      applyMutation.mutate(teamId, {
        onSuccess: () => setSuccessMsg('Bewerbung wurde gesendet.'),
        onError: handleConsentError,
      })
    })
  }

  return (
    <motion.div
      initial={{ opacity: 0 }}
      animate={{ opacity: 1 }}
      className="space-y-8 pb-20"
    >
      {/* Consent Modal */}
      {showConsentModal && (
        <ConsentModal
          onAccepted={() => {
            setShowConsentModal(false)
            if (pendingAction) {
              pendingAction()
              setPendingAction(null)
            }
          }}
          onDismiss={() => { setShowConsentModal(false); setPendingAction(null) }}
        />
      )}

      {/* Hero Header */}
      <section className="relative overflow-hidden rounded-lg border-b border-white/5 pb-12">
        <div className="absolute top-0 right-0 p-4">
          <Badge status={tournament.status} className="scale-110" />
        </div>
        <div className="space-y-4">
          <div className="flex items-center gap-3">
             <div className="p-2 rounded-lg bg-primary/10 border border-primary/20">
                <Trophy size={20} className="text-primary" />
             </div>
             <p className="text-[10px] font-bold text-primary uppercase tracking-[0.3em]">Herausforderung</p>
          </div>
          <h1 className="text-4xl md:text-5xl font-bold text-foreground font-display tracking-tight">{tournament.name}</h1>
          {tournament.description && (
            <p className="text-muted italic max-w-3xl leading-relaxed">{tournament.description}</p>
          )}

          <div className="flex flex-wrap items-center gap-x-8 gap-y-3 pt-4">
            <div className="space-y-1">
              <p className="text-[10px] uppercase font-bold text-muted tracking-widest">Team-Format</p>
              <p className="text-sm font-bold text-foreground uppercase">{tournament.team_size} gegen {tournament.team_size}</p>
            </div>
            <div className="space-y-1">
              <p className="text-[10px] uppercase font-bold text-muted tracking-widest">Kader</p>
              <p className="text-sm font-bold text-foreground uppercase">{tournament.teams.length} Teams gemeldet</p>
            </div>
            <div className="space-y-1">
              <p className="text-[10px] uppercase font-bold text-muted tracking-widest">Regelwerk</p>
              <p className="text-sm font-bold text-foreground uppercase">{tournament.bracket_format === 'single_elimination' ? 'Single Elim.' : 'Double Elim.'}</p>
            </div>
            {tournament.tournament_game_mode !== 'standard' && (
              <div className="space-y-1">
                <p className="text-[10px] uppercase font-bold text-primary tracking-widest">Modus</p>
                <p className="text-sm font-bold text-primary uppercase">{GAME_MODE_LABEL[tournament.tournament_game_mode]}</p>
              </div>
            )}
          </div>
        </div>
      </section>

      {/* Action Banner Group */}
      <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
        {/* Success message */}
        {successMsg && (
          <motion.div
            initial={{ height: 0, opacity: 0 }}
            animate={{ height: 'auto', opacity: 1 }}
            className="md:col-span-2 flex items-center gap-2 text-green-400 text-sm font-bold uppercase tracking-wider bg-green-500/10 border border-green-500/20 rounded-lg p-4"
          >
            <CheckCircle2 size={16} />
            {successMsg}
          </motion.div>
        )}

        {/* Checkin Banner */}
        {isCheckinPhase && (
          <Card className={`p-6 border-amber-500/20 bg-amber-500/5 ${!hasCheckedIn ? 'animate-pulse' : ''}`}>
            <div className="flex flex-col gap-4">
              <div className="space-y-1">
                <div className="flex items-center gap-2 text-amber-500">
                  <ClipboardCheck size={18} />
                  <span className="text-xs font-bold uppercase tracking-widest">Appell</span>
                </div>
                <h3 className="text-lg font-bold text-foreground uppercase font-display">Check-in aktiv</h3>
                <p className="text-xs text-muted italic">
                  Bestätige deine Anwesenheit, sonst wirst du aus der Arena verbannt.
                </p>
              </div>
              <div className="flex items-center justify-between">
                <div className="text-xs text-muted">
                  <span className="text-foreground font-bold">{checkinStatus?.total_checked_in ?? 0}</span> / {checkinStatus?.total_registered ?? tournament.signups.length} bereit
                </div>
                {isLoggedIn && isUserRegistered && (
                  <Button
                    variant={hasCheckedIn ? 'secondary' : 'primary'}
                    size="sm"
                    disabled={hasCheckedIn || checkinMutation.isPending}
                    onClick={handleCheckin}
                  >
                    {hasCheckedIn ? 'Bereit gemeldet' : 'Appell bestätigen'}
                  </Button>
                )}
              </div>
            </div>
          </Card>
        )}

        {/* Offene Einladungen */}
        {isLoggedIn && !myStatus?.team_id && myInvitations && myInvitations.length > 0 && (
          <Card className="p-6 border-primary/20 bg-primary/5">
            <div className="space-y-4">
              <div className="flex items-center gap-2 text-primary">
                <Mail size={18} />
                <h3 className="text-xs font-bold uppercase tracking-widest">Einberufung</h3>
              </div>
              <div className="space-y-3">
                {myInvitations.map((invite) => (
                  <div key={invite.id} className="flex items-center justify-between gap-3 bg-white/5 p-3 rounded-lg border border-white/5">
                    <div>
                      <p className="text-sm font-bold text-foreground uppercase tracking-wide">{invite.team_name ?? `Team #${invite.team_id}`}</p>
                      {invite.expires_at && (
                        <p className="text-[10px] text-muted italic">
                          Ablauf: {new Date(invite.expires_at).toLocaleTimeString('de-DE', { hour: '2-digit', minute: '2-digit' })}
                        </p>
                      )}
                    </div>
                    <div className="flex gap-2">
                      <Button
                        variant="primary"
                        size="sm"
                        disabled={isMutating}
                        onClick={() => acceptInviteMutation.mutate(invite.id, { onSuccess: () => setSuccessMsg('Team beigetreten!') })}
                      >
                        Folgen
                      </Button>
                      <button
                        className="p-2 text-muted hover:text-danger transition-colors"
                        onClick={() => rejectInviteMutation.mutate(invite.id)}
                      >
                        <X size={16} />
                      </button>
                    </div>
                  </div>
                ))}
              </div>
            </div>
          </Card>
        )}
      </div>

      {/* Immersive Tabs */}
      <div className="flex border-b border-white/5 gap-2 overflow-x-auto pb-px">
        {availableTabs.map((tab) => {
          const Icon = tab.icon
          const isActive = activeTab === tab.key
          return (
            <button
              key={tab.key}
              onClick={() => setActiveTab(tab.key)}
              className={`flex items-center gap-2 px-6 py-4 text-[10px] font-bold uppercase tracking-[0.2em] transition-all relative ${
                isActive
                  ? 'text-primary'
                  : 'text-muted hover:text-foreground hover:bg-white/5'
              }`}
            >
              <Icon size={14} className={isActive ? 'text-primary' : 'text-muted'} />
              {tab.label}
              {isActive && (
                <motion.div
                  layoutId="activeTab"
                  className="absolute bottom-0 left-0 right-0 h-0.5 bg-primary shadow-[var(--glow-primary)]"
                />
              )}
            </button>
          )
        })}
      </div>

      {/* Tab Content */}
      <motion.div
        key={activeTab}
        initial={{ opacity: 0, y: 5 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ duration: 0.2 }}
      >
        {activeTab === 'übersicht' && (
          <div className="space-y-8">
            <div className="grid grid-cols-1 md:grid-cols-3 gap-8">
              <div className="md:col-span-2 space-y-8">
                <Card className="p-8 space-y-6">
                  <h2 className="text-xl font-bold font-display uppercase tracking-widest text-foreground flex items-center gap-3">
                    <Info size={20} className="text-primary" />
                    Chronologie
                  </h2>
                  <div className="grid grid-cols-1 sm:grid-cols-2 gap-8">
                    {tournament.registration_start && (
                      <div className="space-y-2">
                        <p className="text-[10px] uppercase font-bold text-muted tracking-widest">Rekrutierung beginnt</p>
                        <p className="text-lg font-bold text-foreground">
                          {new Date(tournament.registration_start).toLocaleString('de-DE', { day: '2-digit', month: '2-digit', hour: '2-digit', minute: '2-digit' })}
                        </p>
                      </div>
                    )}
                    {tournament.registration_end && (
                      <div className="space-y-2">
                        <p className="text-[10px] uppercase font-bold text-primary tracking-widest">Prüfungsbeginn</p>
                        <p className="text-lg font-bold text-primary">
                          {new Date(tournament.registration_end).toLocaleString('de-DE', { day: '2-digit', month: '2-digit', hour: '2-digit', minute: '2-digit' })}
                        </p>
                      </div>
                    )}
                  </div>
                </Card>

                {isSignupOpen && (
                   <Card className="p-8 border-primary/10 bg-primary/5 space-y-6">
                     <div className="flex items-center justify-between">
                        <h2 className="text-xl font-bold font-display uppercase tracking-widest text-foreground">Deine Einschreibung</h2>
                        <Button variant="ghost" size="sm" onClick={() => setActiveTab('teams')}>
                          Zum Kader <Users size={14} />
                        </Button>
                     </div>

                     {!isLoggedIn ? (
                        <div className="text-center py-6 space-y-4">
                           <p className="text-muted italic">Du musst dich erst identifizieren, um teilzunehmen.</p>
                           <LoginButton />
                        </div>
                     ) : !userTeam && !userHasSoloSignup ? (
                        <div className="space-y-4">
                          <p className="text-muted italic text-sm">Melde dich als einzelner Krieger an oder gründe eine Legion.</p>
                          <div className="flex flex-wrap gap-4">
                            <Button variant="primary" onClick={handleSignupSolo} disabled={signupSoloMutation.isPending}>
                              Solo einschreiben
                            </Button>
                            <Button variant="secondary" onClick={() => setActiveTab('teams')}>
                              Legion gründen
                            </Button>
                          </div>
                        </div>
                     ) : userHasSoloSignup && !userTeam ? (
                        <div className="flex items-center justify-between p-4 rounded-lg border border-green-500/20 bg-green-500/5">
                           <div className="flex items-center gap-3 text-green-400">
                              <CheckCircle2 size={20} />
                              <span className="font-bold uppercase tracking-widest text-sm">Solo eingetragen</span>
                           </div>
                           <Button
                              variant="ghost"
                              size="sm"
                              onClick={handleWithdrawSolo}
                              disabled={isMutating}
                              className="text-red-400 border border-red-500/20 hover:bg-red-500/10"
                            >
                              Austragen
                           </Button>
                        </div>
                     ) : userTeam ? (
                        <div className="flex items-center justify-between p-4 rounded-lg border border-primary/20 bg-primary/5">
                           <div className="flex items-center gap-3 text-primary">
                              <Shield size={20} />
                              <div>
                                <p className="text-[10px] uppercase font-bold tracking-widest opacity-60 leading-none mb-1">Eingetragen mit</p>
                                <p className="font-bold uppercase tracking-wider text-sm">{userTeam.name}</p>
                              </div>
                           </div>
                           <Button variant="primary" size="sm" onClick={() => setActiveTab('teams')}>
                              Verwalten
                           </Button>
                        </div>
                     ) : null}
                   </Card>
                )}
              </div>

              <div className="space-y-6">
                <Card className="p-6">
                  <h3 className="text-xs font-bold uppercase tracking-widest text-muted mb-4 border-b border-white/5 pb-2">Statusberichte</h3>
                  <div className="space-y-4">
                    {tournament.invite_mode === 'window' && (
                       <div className="space-y-1">
                          <p className="text-[10px] uppercase font-bold text-amber-500 tracking-widest">Rekrutierungs-Fenster</p>
                          <p className="text-xs text-foreground italic">Nur zu bestimmten Zeiten offen.</p>
                       </div>
                    )}
                    <div className="space-y-1">
                       <p className="text-[10px] uppercase font-bold text-muted tracking-widest">Arena-Regeln</p>
                       <p className="text-xs text-foreground italic">Unsportlichkeit führt zum Ausschluss.</p>
                    </div>
                    {tournament.rules && (
                       <div className="pt-4 border-t border-white/5">
                          <button
                            onClick={() => setActiveTab('kodex')}
                            className="flex items-center gap-2 text-[10px] font-bold text-primary uppercase tracking-widest hover:text-primary-hover transition-colors"
                          >
                            <Book size={14} />
                            Vollständiges Regelwerk lesen
                          </button>
                       </div>
                    )}
                  </div>
                </Card>
              </div>
            </div>
          </div>
        )}

        {activeTab === 'kodex' && (
          <div className="max-w-4xl mx-auto space-y-8">
            <div className="flex items-center gap-3 border-l-2 border-primary pl-4 py-1">
               <Book size={24} className="text-primary" />
               <h2 className="text-2xl font-bold tracking-widest text-foreground uppercase font-display">Die Gesetze der Arena</h2>
            </div>
            
            <Card className="p-8 md:p-12 border-white/5 bg-white/[0.02] relative overflow-hidden">
               <div className="absolute top-0 right-0 p-8 opacity-[0.03] pointer-events-none">
                  <Shield size={200} />
               </div>
               
               <div className="relative z-10 prose prose-invert prose-amber max-w-none">
                  {tournament.rules ? (
                    <ReactMarkdown>{tournament.rules}</ReactMarkdown>
                  ) : (
                    <div className="text-center py-20 opacity-40 italic">
                       <ScrollText size={48} className="mx-auto mb-4 opacity-20" />
                       <p className="text-xl">Keine spezifischen Gesetze für dieses Turnier verkündet.</p>
                       <p className="text-sm mt-2">Es gelten die allgemeinen Bestimmungen des Regelwerks.</p>
                    </div>
                  )}
               </div>
            </Card>
          </div>
        )}

        {activeTab === 'teams' && (
          <div className="space-y-8">
            <div className="grid grid-cols-1 lg:grid-cols-3 gap-8">
              {/* Sidebar: Your Team / Create Team */}
              <div className="space-y-6">
                 {isSignupOpen && isLoggedIn && !userTeam && !userHasSoloSignup && (
                    <Card className="p-6 space-y-6 border-primary/10">
                      <h3 className="text-sm font-bold uppercase tracking-widest text-foreground">Legion gründen</h3>
                      <div className="space-y-4">
                        <Button variant="primary" className="w-full" onClick={() => setShowCreateTeam(!showCreateTeam)} disabled={createTeamMutation.isPending}>
                          Team gründen
                        </Button>
                        {showCreateTeam && (
                          <motion.form
                            initial={{ height: 0, opacity: 0 }}
                            animate={{ height: 'auto', opacity: 1 }}
                            onSubmit={handleCreateTeam}
                            className="space-y-3 pt-2"
                          >
                            <input
                              type="text"
                              value={teamName}
                              onChange={(e) => setTeamName(e.target.value)}
                              placeholder="Name der Legion..."
                              className="w-full bg-white/5 border border-white/10 rounded-lg px-4 py-3 text-sm focus:outline-none focus:border-primary/50"
                            />
                            <div className="flex gap-2">
                              <Button type="submit" size="sm" className="flex-1" disabled={createTeamMutation.isPending}>
                                Beschwören
                              </Button>
                              <Button variant="ghost" size="sm" onClick={() => setShowCreateTeam(false)}>
                                Abbrechen
                              </Button>
                            </div>
                          </motion.form>
                        )}
                      </div>
                    </Card>
                 )}

                 {userTeam && (
                    <Card className="p-6 space-y-6 border-primary/30 relative overflow-hidden">
                      <div className="absolute top-0 right-0 p-2">
                        <Shield size={24} className="text-primary opacity-20" />
                      </div>
                      <div>
                        <p className="text-[10px] font-bold text-primary uppercase tracking-widest mb-1">Deine Legion</p>
                        <h3 className="text-xl font-bold uppercase tracking-tight text-foreground">{userTeam.name}</h3>
                      </div>

                      <div className="space-y-3">
                        {userTeam.members.map((m, i) => (
                          <div key={i} className="flex items-center justify-between text-xs border-b border-white/5 pb-2 last:border-0">
                            <span className="font-bold text-foreground uppercase tracking-wide">{safePublicName(m.discord_name)}</span>
                            {m.role === 'captain' && <span className="text-[10px] text-primary font-bold uppercase">Captain</span>}
                          </div>
                        ))}
                      </div>

                      {isRegistration && (
                         <Button
                          variant="ghost"
                          size="sm"
                          className="w-full text-red-400 border border-red-500/20 hover:bg-red-500/10"
                          onClick={() => handleLeaveTeam(userTeam.id)}
                          disabled={isMutating || (isUserCaptain && userTeam.members.length > 1)}
                        >
                          Legion verlassen
                         </Button>
                      )}
                    </Card>
                 )}
              </div>

              {/* Team List */}
              <div className="lg:col-span-2 space-y-6">
                {/* Registered players overview */}
                {tournament.signups.length > 0 && (
                  <div className="space-y-3">
                    <div className="flex items-center justify-between">
                      <h3 className="text-sm font-bold uppercase tracking-widest text-muted">Eingetragene Spieler</h3>
                      <span className="text-xs font-bold text-foreground bg-white/5 px-2 py-1 rounded-lg border border-white/5">
                        {tournament.signups.length}
                      </span>
                    </div>
                    <div className="grid grid-cols-1 sm:grid-cols-2 gap-2">
                      {tournament.signups.map((signup) => {
                        const teamName = signup.team_id != null
                          ? tournament.teams.find((t) => t.id === signup.team_id)?.name
                          : null
                        return (
                          <div
                            key={signup.id}
                            className="flex items-center gap-3 p-2.5 rounded-lg bg-white/5 border border-white/5"
                          >
                            <User size={13} className="text-muted shrink-0" />
                            <span className="text-sm text-foreground font-medium truncate flex-1">
                              {safePublicName(signup.discord_name)}
                            </span>
                            {teamName ? (
                              <span className="text-[10px] font-bold text-primary uppercase tracking-wide shrink-0">
                                {teamName}
                              </span>
                            ) : (
                              <span className="text-[10px] font-bold text-muted uppercase tracking-wide shrink-0">
                                Solo
                              </span>
                            )}
                          </div>
                        )
                      })}
                    </div>
                  </div>
                )}

                <div className="flex items-center justify-between">
                  <h3 className="text-sm font-bold uppercase tracking-widest text-muted">Gemeldete Legionen</h3>
                  <span className="text-xs font-bold text-foreground bg-white/5 px-2 py-1 rounded-lg border border-white/5">
                    {tournament.teams.length}
                  </span>
                </div>

                <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
                  {tournament.teams.map((team) => {
                    const recruiting = recruitingLabel(team.recruitment_status)
                    const isMyTeam = userTeam?.id === team.id
                    return (
                      <Card key={team.id} className={`p-5 transition-all group ${isMyTeam ? 'border-primary/50' : 'hover:border-white/20'}`}>
                        <div className="space-y-4">
                          <div className="flex items-start justify-between gap-3">
                            <div>
                              <h4 className="font-bold text-foreground uppercase tracking-wide group-hover:text-primary transition-colors">{team.name}</h4>
                              <p className="text-[10px] text-muted font-bold uppercase mt-1">
                                {team.members.length} / {tournament.team_size} Mitglieder
                              </p>
                            </div>
                            <span className={`text-[10px] px-2 py-0.5 rounded-lg border font-bold uppercase tracking-widest ${recruiting.color}`}>
                              {recruiting.text}
                            </span>
                          </div>

                          <div className="flex flex-wrap gap-2">
                             {team.members.slice(0, 3).map((m, i) => (
                               <div key={i} title={m.discord_name ?? ''} className="w-6 h-6 rounded-lg bg-white/5 border border-white/10 flex items-center justify-center overflow-hidden">
                                  <User size={12} className="text-muted" />
                               </div>
                             ))}
                             {team.members.length > 3 && (
                               <div className="w-6 h-6 rounded-lg bg-white/5 border border-white/10 flex items-center justify-center text-[10px] font-bold text-muted">
                                 +{team.members.length - 3}
                               </div>
                             )}
                          </div>

                          {!isMyTeam && isSignupOpen && (
                            <div className="pt-2">
                               {team.recruitment_status === 'open' ? (
                                 <Button variant="secondary" size="sm" className="w-full" onClick={() => handleJoinTeam(team)}>
                                   Beitreten
                                 </Button>
                               ) : team.recruitment_status === 'application' ? (
                                 <Button variant="outline" size="sm" className="w-full border-amber-500/30 text-amber-500 hover:bg-amber-500/10" onClick={() => handleApply(team.id)}>
                                   Bewerben
                                 </Button>
                               ) : null}
                            </div>
                          )}
                        </div>
                      </Card>
                    )
                  })}
                </div>
              </div>
            </div>
          </div>
        )}

        {activeTab === 'gruppen' && (
          <div className="space-y-12">
            <GroupStandings groups={tournament.groups} teams={tournament.teams} />
            <div className="border-t border-white/5 pt-12">
               <GroupMatchList groups={tournament.groups} teams={tournament.teams} />
            </div>
          </div>
        )}

        {activeTab === 'bracket' && (
          <div className="space-y-12">
            {isUserCaptain && myStatus?.team_id != null && (
              <MyMatchReport
                tournamentId={tournamentId}
                matches={tournament.bracket_matches}
                teams={tournament.teams}
                myTeamId={myStatus.team_id}
              />
            )}
            {tournament.mini_groups.length > 0 && (
              <MiniGroupPanel
                miniGroups={tournament.mini_groups}
                matches={tournament.bracket_matches}
                teams={tournament.teams}
              />
            )}
            <BracketView matches={tournament.bracket_matches} teams={tournament.teams} />
          </div>
        )}

        {activeTab === 'rangliste' && (
           <div className="space-y-12">
              <TournamentRangliste tournament={tournament} />
           </div>
        )}

        {activeTab === 'ergebnisse' && (
          <div className="space-y-6">
            {resultEntries.length === 0 ? (
              <Card className="p-20 text-center opacity-40 italic">
                <ScrollText size={48} className="mx-auto mb-4 opacity-20" />
                <p className="text-xl">Die Annalen sind noch leer.</p>
              </Card>
            ) : (
              <div className="space-y-4">
                <div className="flex items-center gap-3 border-l-2 border-primary pl-4 py-1">
                   <ScrollText size={20} className="text-primary" />
                   <h2 className="text-xl font-bold tracking-widest text-foreground uppercase">Ergebnisprotokoll</h2>
                </div>
                <div className="grid gap-4">
                  {resultEntries.map((entry) => (
                    <Card key={entry.id} className="hover:border-white/20 transition-all group">
                      <div className="flex flex-col md:flex-row md:items-center justify-between gap-4">
                        <div className="space-y-1">
                          <p className="text-[10px] uppercase font-bold text-primary tracking-widest">{entry.title}</p>
                          <div className="flex items-center gap-2 text-foreground font-bold">
                            <span className="text-green-400 uppercase tracking-wide">{entry.winnerName}</span>
                            <span className="text-muted font-normal text-xs uppercase">Besiegt</span>
                            <span className="text-muted uppercase tracking-wide">{entry.loserName}</span>
                          </div>
                        </div>
                        <div className="text-[10px] text-muted font-bold uppercase tracking-widest">
                          {entry.playedAt
                            ? new Date(entry.playedAt).toLocaleString('de-DE', { day: '2-digit', month: '2-digit', hour: '2-digit', minute: '2-digit' })
                            : 'Unbekannte Zeit'}
                        </div>
                      </div>
                    </Card>
                  ))}
                </div>
              </div>
            )}
          </div>
        )}
      </motion.div>

      {/* Confirmation Modal – Team wechseln */}
      {pendingJoinTeam && userTeam && (
        <div className="fixed inset-0 bg-background/90 backdrop-blur-sm flex items-center justify-center z-[100]">
          <motion.div
            initial={{ scale: 0.9, opacity: 0 }}
            animate={{ scale: 1, opacity: 1 }}
            className="glass-card p-8 max-w-sm w-full mx-4 shadow-2xl border-primary/20"
          >
            <h3 className="text-xl font-bold text-foreground font-display uppercase tracking-widest mb-4">Loyalität wechseln?</h3>
            <p className="text-sm text-muted italic mb-8 leading-relaxed">
              Du bist bereits Teil der Legion <span className="text-primary font-bold">"{userTeam.name}"</span>.
              Willst du ihr den Rücken kehren und dich <span className="text-foreground font-bold">"{pendingJoinTeam.name}"</span> anschließen?
            </p>
            <div className="flex gap-4">
              <Button variant="ghost" className="flex-1" onClick={() => setPendingJoinTeam(null)} disabled={joinTeamMutation.isPending}>Bleiben</Button>
              <Button variant="primary" className="flex-1" disabled={joinTeamMutation.isPending}
                onClick={() => joinTeamMutation.mutate({ tournamentId, teamId: pendingJoinTeam.id }, { onSettled: () => setPendingJoinTeam(null) })}>
                Wechseln
              </Button>
            </div>
          </motion.div>
        </div>
      )}
    </motion.div>
  )
}

function TournamentRangliste({ tournament }: { tournament: TournamentDetailPublic }) {
  const bracketEntries = deriveBracketPlacements(tournament.bracket_matches, tournament.teams)
  const hasGroups = tournament.groups.length > 0

  if (bracketEntries.length === 0 && !hasGroups) {
    return (
      <Card className="p-20 text-center opacity-40 italic">
        <BarChart2 size={48} className="mx-auto text-muted mb-4 opacity-20" />
        <p className="text-xl">Noch keine Rangordnung festgelegt.</p>
      </Card>
    )
  }

  return (
    <div className="space-y-12">
      {bracketEntries.length > 0 && (
        <section className="space-y-6">
          <div className="flex items-center gap-3 border-l-2 border-primary pl-4 py-1">
             <Trophy size={20} className="text-primary" />
             <h2 className="text-xl font-bold tracking-widest text-foreground uppercase">Abschluss-Platzierungen</h2>
          </div>
          <Card className="p-0 overflow-hidden border-white/5">
            <table className="w-full text-sm">
              <thead>
                <tr className="bg-white/5 text-left border-b border-white/10">
                  <th className="px-6 py-4 font-bold uppercase tracking-widest text-muted text-[10px] w-20 text-center">Rang</th>
                  <th className="px-6 py-4 font-bold uppercase tracking-widest text-muted text-[10px]">Legion</th>
                  <th className="px-6 py-4 font-bold uppercase tracking-widest text-muted text-[10px] text-right">Resultat</th>
                </tr>
              </thead>
              <tbody>
                {bracketEntries.map((entry) => (
                  <tr
                    key={entry.teamId}
                    className={`border-b border-white/5 last:border-0 transition-colors ${
                      entry.position === 1 ? 'bg-primary/5' : ''
                    }`}
                  >
                    <td className="px-6 py-4 text-center">
                       {entry.position === 1 ? <Trophy size={18} className="text-primary mx-auto" /> : <span className="font-bold text-muted">{entry.position}</span>}
                    </td>
                    <td className="px-6 py-4 font-bold text-foreground uppercase tracking-wide">{entry.teamName}</td>
                    <td className="px-6 py-4 text-right">
                      <span className="text-[10px] px-2 py-0.5 rounded-lg border border-primary/20 text-primary font-bold uppercase tracking-widest bg-primary/5">
                        {entry.label}
                      </span>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </Card>
        </section>
      )}

      {hasGroups && (
        <section className="space-y-6">
           <div className="flex items-center gap-3 border-l-2 border-primary pl-4 py-1">
             <BarChart2 size={20} className="text-primary" />
             <h2 className="text-xl font-bold tracking-widest text-foreground uppercase">Gruppenwertung</h2>
          </div>
          <Card className="p-0 overflow-hidden border-white/5">
            <table className="w-full text-sm">
              <thead>
                <tr className="bg-white/5 text-left border-b border-white/10">
                  <th className="px-6 py-4 font-bold uppercase tracking-widest text-muted text-[10px] w-20 text-center">#</th>
                  <th className="px-6 py-4 font-bold uppercase tracking-widest text-muted text-[10px]">Legion</th>
                  <th className="px-6 py-4 font-bold uppercase tracking-widest text-muted text-[10px] text-center">S</th>
                  <th className="px-6 py-4 font-bold uppercase tracking-widest text-muted text-[10px] text-center">N</th>
                  <th className="px-6 py-4 font-bold uppercase tracking-widest text-muted text-[10px] text-right">Pkt</th>
                </tr>
              </thead>
              <tbody>
                {tournament.groups
                  .flatMap((g) => g.teams.map((t) => ({ ...t, groupName: g.name })))
                  .sort((a, b) => b.points !== a.points ? b.points - a.points : b.wins - a.wins)
                  .map((team, idx) => (
                    <tr key={`${team.team_id}-${team.groupName}`} className="border-b border-white/5 last:border-0 hover:bg-white/[0.02] transition-colors">
                      <td className="px-6 py-4 text-center text-muted font-bold">{idx + 1}</td>
                      <td className="px-6 py-4 font-bold text-foreground uppercase tracking-wide">{team.team_name}</td>
                      <td className="px-6 py-4 text-center text-muted font-medium">{team.wins}</td>
                      <td className="px-6 py-4 text-center text-muted font-medium">{team.losses}</td>
                      <td className="px-6 py-4 text-right font-bold text-primary">{team.points}</td>
                    </tr>
                  ))}
              </tbody>
            </table>
          </Card>
        </section>
      )}
    </div>
  )
}
