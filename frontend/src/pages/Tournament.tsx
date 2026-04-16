import { useState, useEffect } from 'react'
import type { FormEvent } from 'react'
import { useParams, Link } from 'react-router-dom'
import {
  useTournament, useMyTournamentStatus, useCreateTeam, useJoinTeam, useSignupSolo,
  useWithdrawSolo, useLeaveTeam, useInviteBySignup, useCheckin, useCheckinStatus,
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
import {
  Trophy, Users, LayoutGrid, GitBranch, Plus, UserPlus, AlertCircle, Shield, X, Info,
  ChevronDown, ChevronUp, ScrollText, CheckCircle2, ClipboardCheck, Mail, UserCheck, BarChart2,
} from 'lucide-react'
import type { TeamPublic, BracketMatch, GroupMatch } from '@/types/tournament'
import { ApiError } from '@/api/client'

type Tab = 'übersicht' | 'gruppen' | 'bracket' | 'teams' | 'ergebnisse' | 'rangliste'

const ALL_TABS: { key: Tab; label: string; icon: typeof Trophy }[] = [
  { key: 'übersicht', label: 'Übersicht', icon: Trophy },
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
  const [showSoloTable, setShowSoloTable] = useState(false)
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
  const inviteBySignupMutation = useInviteBySignup(tournamentId)
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

  useEffect(() => {
    if (!tournament) return
    const allowedTabs = ALL_TABS.filter(
      (tab) => showResultsTab || (tab.key !== 'ergebnisse' && tab.key !== 'rangliste')
    )
    if (!allowedTabs.some((tab) => tab.key === activeTab)) {
      setActiveTab('übersicht')
    }
  }, [activeTab, showResultsTab, tournament])

  if (isLoading) return <LoadingSpinner />
  if (!tournament) {
    return (
      <Card className="text-center py-10">
        <p className="text-muted">Turnier nicht gefunden</p>
      </Card>
    )
  }

  const isRegistration = tournament.status === 'registration'
  const isCheckinPhase = tournament.status === 'checkin'

  // User state from status endpoint (no discord_id scanning needed)
  const userTeam = myStatus?.team_id != null
    ? tournament.teams.find((t) => t.id === myStatus.team_id) ?? null
    : null
  const userSignupId = myStatus?.signup_id ?? null
  const userHasSoloSignup = userSignupId !== null && !myStatus?.team_id
  const isUserCaptain = myStatus?.is_captain ?? false
  const teamIsFull = userTeam != null && userTeam.members.length >= tournament.team_size
  const isUserRegistered = Boolean(userTeam || userHasSoloSignup)
  const hasCheckedIn = myStatus?.is_checked_in ?? false

  // Invite window check
  const canInvite = (() => {
    if (!isUserCaptain || !userTeam || teamIsFull) return false
    const mode = tournament.invite_mode
    if (mode === 'never') return false
    if (mode === 'always') return true
    if (mode === 'window') {
      const now = Date.now()
      const start = tournament.invite_window_start ? new Date(tournament.invite_window_start).getTime() : null
      const end = tournament.invite_window_end ? new Date(tournament.invite_window_end).getTime() : null
      return (!start || now >= start) && (!end || now <= end)
    }
    return false
  })()

  const openSoloSignups = tournament.signups.filter((s) => s.team_id === null)
  const resultEntries = tournament
    ? [
        ...buildGroupResultEntries(tournament.groups, tournament.teams),
        ...buildBracketResultEntries(tournament.bracket_matches, tournament.teams),
      ].sort((left, right) => left.sortTime - right.sortTime || left.id.localeCompare(right.id))
    : []

  const mutationError =
    createTeamMutation.error ||
    joinTeamMutation.error ||
    signupSoloMutation.error ||
    withdrawSoloMutation.error ||
    inviteBySignupMutation.error ||
    leaveTeamMutation.error ||
    applyMutation.error

  const isMutating =
    createTeamMutation.isPending ||
    joinTeamMutation.isPending ||
    signupSoloMutation.isPending ||
    withdrawSoloMutation.isPending ||
    inviteBySignupMutation.isPending ||
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

  const handleInviteBySignup = (teamId: number, signupId: number) => {
    inviteBySignupMutation.mutate({ teamId, signupId }, {
      onSuccess: (result) => {
        setSuccessMsg(result.status === 'auto_accepted' ? 'Spieler wurde direkt aufgenommen!' : 'Einladung gesendet.')
      },
    })
  }

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
    <div className="space-y-6">
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

      {/* Success message */}
      {successMsg && (
        <div className="flex items-center gap-2 text-green-400 text-sm bg-green-500/10 border border-green-500/20 rounded-lg p-3">
          <span>{successMsg}</span>
        </div>
      )}

      {/* Pick-Window-Hinweis */}
      {isRegistration && tournament.invite_mode === 'window' && (
        <Card className="p-3 border-amber-500/20 bg-amber-500/5 text-sm text-amber-300">
          {tournament.invite_window_start && tournament.invite_window_end ? (
            <span>
              Einladungen möglich:{' '}
              {new Date(tournament.invite_window_start).toLocaleString('de-DE', { day: '2-digit', month: '2-digit', hour: '2-digit', minute: '2-digit' })}
              {' '}–{' '}
              {new Date(tournament.invite_window_end).toLocaleString('de-DE', { day: '2-digit', month: '2-digit', hour: '2-digit', minute: '2-digit' })}
            </span>
          ) : (
            <span>Pick-Fenster aktiv — Einladungen nur in einem bestimmten Zeitraum erlaubt.</span>
          )}
        </Card>
      )}
      {isRegistration && tournament.invite_mode === 'never' && (
        <Card className="p-3 border-border/30 bg-background/40 text-sm text-muted">
          Teams werden beim Turnier-Start automatisch zusammengestellt.
        </Card>
      )}

      {/* Checkin Banner */}
      {isCheckinPhase && (
        <Card className="p-5 border-amber-500/20 bg-amber-500/5">
          <div className="flex flex-col gap-4 md:flex-row md:items-center md:justify-between">
            <div className="space-y-2">
              <div className="flex items-center gap-2 text-amber-300">
                <ClipboardCheck size={18} />
                <span className="text-sm font-semibold uppercase tracking-wide">Check-in aktiv</span>
              </div>
              <p className="text-sm text-foreground">
                {checkinStatus?.total_checked_in ?? 0} von {checkinStatus?.total_registered ?? tournament.signups.length} Spielern sind eingecheckt.
              </p>
              {hasCheckedIn && (
                <div className="inline-flex items-center gap-2 rounded-full bg-green-500/15 px-3 py-1 text-sm text-green-400">
                  <CheckCircle2 size={14} />
                  Check-in bestätigt
                </div>
              )}
            </div>
            {isLoggedIn && isUserRegistered ? (
              <Button
                variant="primary"
                size="sm"
                disabled={hasCheckedIn || checkinMutation.isPending}
                onClick={handleCheckin}
              >
                <ClipboardCheck size={14} />
                {hasCheckedIn ? 'Eingecheckt' : checkinMutation.isPending ? 'Checkt ein...' : 'Jetzt einchecken'}
              </Button>
            ) : (
              <p className="text-sm text-muted">Nur angemeldete Spieler können sich einchecken.</p>
            )}
          </div>
        </Card>
      )}

      {/* Offene Einladungen */}
      {isLoggedIn && !myStatus?.team_id && myInvitations && myInvitations.length > 0 && (
        <Card className="p-4 border-primary/20 bg-primary/5">
          <div className="flex items-center gap-2 mb-3">
            <Mail size={16} className="text-primary" />
            <h3 className="text-sm font-semibold text-foreground">Offene Einladungen</h3>
          </div>
          <div className="space-y-2">
            {myInvitations.map((invite) => (
              <div key={invite.id} className="flex items-center justify-between gap-3">
                <div>
                  <p className="text-sm font-medium text-foreground">{invite.team_name ?? `Team #${invite.team_id}`}</p>
                  {invite.expires_at && (
                    <p className="text-xs text-muted">
                      Läuft ab: {new Date(invite.expires_at).toLocaleString('de-DE', { day: '2-digit', month: '2-digit', hour: '2-digit', minute: '2-digit' })}
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
                    <UserCheck size={13} />
                    Annehmen
                  </Button>
                  <Button
                    variant="ghost"
                    size="sm"
                    disabled={isMutating}
                    onClick={() => rejectInviteMutation.mutate(invite.id)}
                  >
                    <X size={13} />
                    Ablehnen
                  </Button>
                </div>
              </div>
            ))}
          </div>
        </Card>
      )}

      {/* Tabs */}
      <div className="flex border-b border-border">
        {availableTabs.map((tab) => {
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
                <span className="relative group" title="Teams spielen in Gruppen gegeneinander (Round-Robin). Die besten 2 jeder Gruppe kommen weiter ins Bracket.">
                  <Info size={13} className="text-muted hover:text-foreground transition-colors" />
                  <span className="pointer-events-none absolute bottom-full left-1/2 -translate-x-1/2 mb-2 w-64 rounded-lg bg-card border border-border px-3 py-2 text-xs text-foreground opacity-0 group-hover:opacity-100 transition-opacity z-10 shadow-lg">
                    Teams spielen in Gruppen gegeneinander (Round-Robin). Die besten 2 jeder Gruppe kommen weiter ins Bracket.
                  </span>
                </span>
              )}
              {tab.key === 'bracket' && (
                <span className="relative group" title="K.O.-Runde: Wer verliert, scheidet aus. Wer gewinnt, kommt eine Runde weiter bis zum Finale.">
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
          <div className="space-y-4">
            <Card className="p-6">
              <h2 className="text-lg font-semibold text-foreground mb-4">Turnier-Informationen</h2>
              <div className="grid grid-cols-1 sm:grid-cols-2 gap-4 text-sm">
                {tournament.registration_start && (
                  <div>
                    <span className="text-muted">Anmeldung Start:</span>
                    <span className="ml-2 text-foreground">
                      {new Date(tournament.registration_start).toLocaleString('de-DE', { day: '2-digit', month: '2-digit', year: 'numeric', hour: '2-digit', minute: '2-digit' })}
                    </span>
                  </div>
                )}
                {tournament.registration_end && (
                  <div>
                    <span className="text-muted">Anmeldung Ende:</span>
                    <span className="ml-2 text-foreground">
                      {new Date(tournament.registration_end).toLocaleString('de-DE', { day: '2-digit', month: '2-digit', year: 'numeric', hour: '2-digit', minute: '2-digit' })}
                    </span>
                  </div>
                )}
              </div>
            </Card>

            {isRegistration && (
              <Card className="p-4">
                <div className="flex items-center justify-between gap-3 mb-3">
                  <div>
                    <h3 className="text-sm font-semibold text-foreground">Anmeldung</h3>
                    <p className="text-sm text-muted">
                      Direkter Einstieg für Solo-Anmeldung. Teams bleiben separat im Tab `Teams`.
                    </p>
                  </div>
                  <Button variant="ghost" size="sm" onClick={() => setActiveTab('teams')}>
                    <Users size={14} />
                    Zu Teams
                  </Button>
                </div>

                {mutationError && (
                  <div className="flex items-center gap-2 text-red-400 text-sm bg-red-500/10 border border-red-500/20 rounded-lg p-3 mb-3">
                    <AlertCircle size={16} />
                    <span>{mutationError instanceof Error ? mutationError.message : 'Ein Fehler ist aufgetreten'}</span>
                  </div>
                )}

                {!isLoggedIn ? (
                  <p className="text-sm text-muted">
                    Melde dich an, um dich für das Turnier zu registrieren oder ein Team zu erstellen.
                  </p>
                ) : !userTeam && !userHasSoloSignup ? (
                  <div className="flex flex-wrap gap-2">
                    <Button variant="secondary" size="sm" onClick={handleSignupSolo} disabled={isMutating}>
                      <UserPlus size={14} />
                      {signupSoloMutation.isPending ? 'Wird angemeldet...' : 'Für Turnier anmelden'}
                    </Button>
                  </div>
                ) : userHasSoloSignup && !userTeam ? (
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
                    <Button variant="ghost" size="sm" onClick={() => setActiveTab('teams')}>
                      <Users size={14} />
                      Teams ansehen
                    </Button>
                  </div>
                ) : userTeam ? (
                  <div className="space-y-3">
                    <div className="inline-flex items-center gap-2 rounded-full bg-primary/15 px-3 py-1 text-sm text-primary">
                      <Shield size={14} />
                      Angemeldet mit {userTeam.name}
                    </div>
                    <div className="flex flex-wrap gap-2">
                      <Button variant="primary" size="sm" onClick={() => setActiveTab('teams')}>
                        <Users size={14} />
                        Team verwalten
                      </Button>
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
                  </div>
                ) : null}
              </Card>
            )}
          </div>
        )}

        {activeTab === 'teams' && (
          <div className="space-y-4">
            {isRegistration && isLoggedIn && !userTeam && !userHasSoloSignup && (
              <Card className="p-4">
                <h3 className="text-sm font-semibold text-foreground mb-3">Team erstellen</h3>
                <p className="text-sm text-muted mb-3">
                  Team-Anlage und Team-Verwaltung sind hier separat gebündelt.
                </p>
                {mutationError && (
                  <div className="flex items-center gap-2 text-red-400 text-sm bg-red-500/10 border border-red-500/20 rounded-lg p-3 mb-3">
                    <AlertCircle size={16} />
                    <span>{mutationError instanceof Error ? mutationError.message : 'Ein Fehler ist aufgetreten'}</span>
                  </div>
                )}
                <div className="flex flex-wrap gap-2">
                  <Button variant="primary" size="sm" onClick={() => setShowCreateTeam(!showCreateTeam)} disabled={isMutating}>
                    <Plus size={14} />
                    Team erstellen
                  </Button>
                  <Button variant="secondary" size="sm" onClick={handleSignupSolo} disabled={isMutating}>
                    <UserPlus size={14} />
                    {signupSoloMutation.isPending ? 'Wird angemeldet...' : 'Für Turnier anmelden'}
                  </Button>
                </div>
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
                    <Button type="submit" variant="primary" size="sm" disabled={createTeamMutation.isPending || !teamName.trim()}>
                      {createTeamMutation.isPending ? 'Erstellt...' : 'Erstellen'}
                    </Button>
                    <Button type="button" variant="ghost" size="sm" onClick={() => { setShowCreateTeam(false); setTeamName('') }}>
                      Abbrechen
                    </Button>
                  </form>
                )}
              </Card>
            )}

            {/* Solo signup status */}
            {isRegistration && isLoggedIn && userHasSoloSignup && !userTeam && (
              <Card className="p-4">
                <h3 className="text-sm font-semibold text-foreground mb-3">Solo-Anmeldung</h3>
                {mutationError && (
                  <div className="flex items-center gap-2 text-red-400 text-sm bg-red-500/10 border border-red-500/20 rounded-lg p-3 mb-3">
                    <AlertCircle size={16} />
                    <span>{mutationError instanceof Error ? mutationError.message : 'Fehler'}</span>
                  </div>
                )}
                <div className="flex items-center gap-3 flex-wrap">
                  <span className="inline-flex items-center gap-1.5 px-3 py-1 rounded-full text-sm font-medium bg-green-600 text-white">
                    ✓ Solo eingetragen
                  </span>
                  <Button variant="ghost" size="sm" onClick={handleWithdrawSolo} disabled={isMutating}
                    className="text-red-400 border border-red-500/40 hover:bg-red-500/10">
                    {withdrawSoloMutation.isPending ? 'Wird ausgetragen...' : 'Austragen'}
                  </Button>
                </div>
              </Card>
            )}

            {/* User's current team */}
            {userTeam && (
              <Card className="p-4 border-primary/30">
                <div className="flex items-center justify-between mb-2">
                  <div className="flex items-center gap-2">
                    <Shield size={16} className="text-primary" />
                    <span className="text-sm font-semibold text-primary">Dein Team</span>
                  </div>
                  {isRegistration && (
                    <Button variant="ghost" size="sm" onClick={() => handleLeaveTeam(userTeam.id)}
                      disabled={isMutating || (isUserCaptain && userTeam.members.length > 1)}
                      title={isUserCaptain && userTeam.members.length > 1 ? 'Übergib zuerst die Captain-Rolle' : undefined}
                      className="text-red-400 border border-red-500/40 hover:bg-red-500/10">
                      {leaveTeamMutation.isPending ? 'Verlasse...' : 'Team verlassen'}
                    </Button>
                  )}
                </div>
                <h3 className="font-medium text-foreground">{userTeam.name}</h3>
                <div className="mt-2 space-y-1">
                  {userTeam.members.map((m, i) => (
                    <div key={i} className="flex items-center gap-2 text-sm">
                      <span className="text-foreground flex-1">{safePublicName(m.discord_name)}</span>
                      {m.role === 'captain' && <span className="text-xs text-primary font-medium">Captain</span>}
                      <span className="text-xs text-muted">{m.rank ?? '—'}</span>
                    </div>
                  ))}
                </div>

                {/* Captain: Solo-Spieler einladen */}
                {isUserCaptain && isRegistration && canInvite && openSoloSignups.length > 0 && (
                  <div className="mt-4 border-t border-border pt-3">
                    <button
                      className="flex items-center gap-1.5 text-sm font-medium text-foreground hover:text-primary transition-colors"
                      onClick={() => setShowSoloTable((v) => !v)}
                    >
                      <UserPlus size={14} />
                      Solo-Spieler einladen
                      {showSoloTable ? <ChevronUp size={14} /> : <ChevronDown size={14} />}
                    </button>
                    {showSoloTable && (
                      <div className="mt-3 overflow-x-auto">
                        <table className="w-full text-sm">
                          <thead>
                            <tr className="text-left text-xs text-muted border-b border-border">
                              <th className="pb-2 pr-4">Name</th>
                              <th className="pb-2 pr-4">Rang</th>
                              <th className="pb-2 text-right">Aktion</th>
                            </tr>
                          </thead>
                          <tbody>
                            {openSoloSignups.map((signup) => (
                              <tr key={signup.id} className="border-b border-border/50 last:border-0">
                                <td className="py-2 pr-4 font-medium text-foreground">{safePublicName(signup.discord_name)}</td>
                                <td className="py-2 pr-4 text-muted text-xs">
                                  {signup.rank
                                    ? `${signup.rank} · ${signup.rank_score}`
                                    : `Score ${signup.rank_score}`}
                                </td>
                                <td className="py-2 text-right">
                                  <Button
                                    variant="secondary"
                                    size="sm"
                                    disabled={isMutating}
                                    onClick={() => handleInviteBySignup(userTeam.id, signup.id)}
                                  >
                                    {inviteBySignupMutation.isPending ? 'Eingeladen...' : 'Einladen'}
                                  </Button>
                                </td>
                              </tr>
                            ))}
                          </tbody>
                        </table>
                      </div>
                    )}
                  </div>
                )}
                {isUserCaptain && isRegistration && !canInvite && tournament.invite_mode !== 'always' && (
                  <p className="mt-3 text-xs text-muted pt-2 border-t border-border">
                    {tournament.invite_mode === 'never'
                      ? 'Einladungen sind für dieses Turnier deaktiviert.'
                      : 'Das Einladungs-Fenster ist aktuell nicht geöffnet.'}
                  </p>
                )}
              </Card>
            )}

            {/* Error outside registration context */}
            {mutationError && !isRegistration && (
              <div className="flex items-center gap-2 text-red-400 text-sm bg-red-500/10 border border-red-500/20 rounded-lg p-3">
                <AlertCircle size={16} />
                <span>{mutationError instanceof Error ? mutationError.message : 'Ein Fehler ist aufgetreten'}</span>
              </div>
            )}

            {/* Solo Signups Table (wenn kein Team aktiv) */}
            {openSoloSignups.length > 0 && !isUserCaptain && (
              <Card className="p-4">
                <h3 className="text-sm font-semibold text-foreground mb-3">Offene Solo-Anmeldungen</h3>
                <div className="overflow-x-auto">
                  <table className="w-full text-sm">
                    <thead>
                      <tr className="text-left text-xs text-muted border-b border-border">
                        <th className="pb-2 pr-4">Name</th>
                        <th className="pb-2 pr-4 hidden sm:table-cell">Rang</th>
                      </tr>
                    </thead>
                    <tbody>
                      {openSoloSignups.map((signup) => (
                        <tr key={signup.id} className="border-b border-border/50 last:border-0">
                          <td className="py-2 pr-4 font-medium text-foreground">
                            {signup.discord_name?.trim() ? (
                              <Link to={`/spieler/${encodeURIComponent(signup.discord_name)}`}
                                className="hover:text-primary transition-colors">
                                {signup.discord_name}
                              </Link>
                            ) : (
                              safePublicName(signup.discord_name)
                            )}
                          </td>
                          <td className="py-2 pr-4 text-muted text-xs hidden sm:table-cell">
                            {signup.rank ?? '—'}
                          </td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </div>
              </Card>
            )}

            {/* Team List */}
            <div className="grid gap-3">
              {tournament.teams.length > 0 ? tournament.teams.map((team) => {
                const recruiting = recruitingLabel(team.recruitment_status)
                const isCaptainTeam = userTeam?.id === team.id
                const isFull = team.members.length >= tournament.team_size
                const showJoin = isRegistration && isLoggedIn && !isCaptainTeam && team.recruitment_status === 'open' && !isFull
                const showApply = isRegistration && isLoggedIn && !isCaptainTeam && team.recruitment_status === 'application' && !isFull && !userTeam
                return (
                  <Card key={team.id} className={`p-4 ${isCaptainTeam ? 'border-primary/30' : ''}`}>
                    <div className="flex items-start justify-between gap-3">
                      <div className="flex-1 min-w-0">
                        <div className="flex items-center gap-2 flex-wrap">
                          <h3 className="font-medium text-foreground">{team.name}</h3>
                          <span className="text-sm text-muted">{team.members.length}/{tournament.team_size}</span>
                          <span className={`text-xs px-2 py-0.5 rounded-full border font-medium ${recruiting.color}`}>
                            {recruiting.text}
                          </span>
                          {team.has_pending_applications && isUserCaptain && isCaptainTeam && (
                            <span className="text-xs px-2 py-0.5 rounded-full border border-amber-500/30 bg-amber-500/10 text-amber-400 font-medium">
                              Neue Bewerbungen
                            </span>
                          )}
                        </div>
                        <div className="mt-2 flex flex-wrap gap-x-4 gap-y-1">
                          {team.members.map((m, i) => (
                            <span key={i} className="text-xs text-muted">
                              {m.discord_name?.trim() ? (
                                <Link to={`/spieler/${encodeURIComponent(m.discord_name)}`}
                                  className="hover:text-primary transition-colors">
                                  {m.discord_name}
                                </Link>
                              ) : (
                                safePublicName(m.discord_name)
                              )}
                              {m.role === 'captain' && <span className="ml-1 text-primary">(C)</span>}
                              <span className="ml-1 opacity-60">[{m.rank ?? '—'}]</span>
                            </span>
                          ))}
                        </div>
                      </div>
                      <div className="flex gap-2 flex-shrink-0">
                        {showJoin && (
                          <Button variant="secondary" size="sm" onClick={() => handleJoinTeam(team)} disabled={isMutating}>
                            <UserPlus size={14} />
                            Beitreten
                          </Button>
                        )}
                        {showApply && (
                          <Button variant="ghost" size="sm" onClick={() => handleApply(team.id)} disabled={isMutating}
                            className="border border-amber-500/40 text-amber-400 hover:bg-amber-500/10">
                            Bewerben
                          </Button>
                        )}
                      </div>
                    </div>
                  </Card>
                )
              }) : (
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

        {activeTab === 'rangliste' && (() => {
          const bracketEntries = deriveBracketPlacements(tournament.bracket_matches, tournament.teams)
          const hasGroups = tournament.groups.length > 0

          if (bracketEntries.length === 0 && !hasGroups) {
            return (
              <Card className="p-8 text-center">
                <BarChart2 size={36} className="mx-auto text-muted mb-3" />
                <p className="text-muted">Noch keine Rangliste verfügbar.</p>
                <p className="text-xs text-muted mt-1">Platzierungen werden nach Start der Gruppenphase berechnet.</p>
              </Card>
            )
          }

          return (
            <div className="space-y-6">
              {bracketEntries.length > 0 && (
                <div>
                  <h2 className="text-lg font-semibold text-foreground mb-3">Abschluss-Platzierungen</h2>
                  <Card className="overflow-hidden">
                    <table className="w-full text-sm">
                      <thead>
                        <tr className="border-b border-border bg-background/50">
                          <th className="px-4 py-3 text-left font-medium text-muted w-12">#</th>
                          <th className="px-4 py-3 text-left font-medium text-muted">Team</th>
                          <th className="px-4 py-3 text-left font-medium text-muted hidden sm:table-cell">Ergebnis</th>
                        </tr>
                      </thead>
                      <tbody>
                        {bracketEntries.map((entry) => (
                          <tr
                            key={entry.teamId}
                            className={`border-b border-border/50 last:border-0 transition-colors ${
                              entry.position <= 3 ? 'bg-primary/5' : ''
                            }`}
                          >
                            <td className="px-4 py-3">
                              <div className="flex items-center justify-center">
                                {entry.position === 1
                                  ? <Trophy size={16} className="text-yellow-400" />
                                  : entry.position === 2
                                    ? <Trophy size={16} className="text-slate-300" />
                                    : <span className="text-sm font-bold text-muted">{entry.position}</span>}
                              </div>
                            </td>
                            <td className="px-4 py-3 font-medium text-foreground">{entry.teamName}</td>
                            <td className="px-4 py-3 hidden sm:table-cell">
                              <span className="text-xs px-2 py-0.5 rounded-full bg-primary/15 text-primary font-medium">
                                {entry.label}
                              </span>
                            </td>
                          </tr>
                        ))}
                      </tbody>
                    </table>
                  </Card>
                </div>
              )}

              {hasGroups && bracketEntries.length === 0 && (
                <div>
                  <h2 className="text-lg font-semibold text-foreground mb-1">Gruppenrangliste</h2>
                  <p className="text-sm text-muted mb-3">Alle Teams sortiert nach Gruppenphase-Punkten.</p>
                  <Card className="overflow-hidden">
                    <table className="w-full text-sm">
                      <thead>
                        <tr className="border-b border-border bg-background/50">
                          <th className="px-4 py-3 text-left font-medium text-muted w-12">#</th>
                          <th className="px-4 py-3 text-left font-medium text-muted">Team</th>
                          <th className="px-4 py-3 text-center font-medium text-muted">S</th>
                          <th className="px-4 py-3 text-center font-medium text-muted">N</th>
                          <th className="px-4 py-3 text-right font-medium text-muted">Pkt</th>
                          <th className="px-4 py-3 text-left font-medium text-muted hidden sm:table-cell">Gruppe</th>
                        </tr>
                      </thead>
                      <tbody>
                        {tournament.groups
                          .flatMap((g) => g.teams.map((t) => ({ ...t, groupName: g.name })))
                          .sort((a, b) => b.points !== a.points ? b.points - a.points : b.wins - a.wins)
                          .map((team, idx) => (
                            <tr key={`${team.team_id}-${team.groupName}`} className="border-b border-border/50 last:border-0 hover:bg-background/40 transition-colors">
                              <td className="px-4 py-3 text-sm font-bold text-muted">{idx + 1}</td>
                              <td className="px-4 py-3 font-medium text-foreground">{team.team_name}</td>
                              <td className="px-4 py-3 text-center text-muted">{team.wins}</td>
                              <td className="px-4 py-3 text-center text-muted">{team.losses}</td>
                              <td className="px-4 py-3 text-right font-bold text-foreground">{team.points}</td>
                              <td className="px-4 py-3 hidden sm:table-cell">
                                <span className="text-xs px-2 py-0.5 rounded-full bg-border/50 text-muted">{team.groupName}</span>
                              </td>
                            </tr>
                          ))}
                      </tbody>
                    </table>
                  </Card>
                </div>
              )}
            </div>
          )
        })()}

        {activeTab === 'ergebnisse' && (
          <div className="space-y-4">
            {resultEntries.length === 0 ? (
              <Card className="p-6 text-center">
                <ScrollText size={32} className="mx-auto mb-3 text-muted" />
                <p className="text-muted">Noch keine abgeschlossenen Ergebnisse vorhanden</p>
              </Card>
            ) : (
              <>
                <div>
                  <h2 className="text-lg font-semibold text-foreground">Ergebnisübersicht</h2>
                  <p className="mt-1 text-sm text-muted">Alle abgeschlossenen Gruppen- und Bracket-Matches in zeitlicher Reihenfolge.</p>
                </div>
                <div className="space-y-3">
                  {resultEntries.map((entry) => (
                    <Card key={entry.id} className="p-4">
                      <div className="flex flex-col gap-3 md:flex-row md:items-center md:justify-between">
                        <div>
                          <p className="text-xs uppercase tracking-wide text-primary">{entry.title}</p>
                          <p className="mt-1 text-sm text-foreground">
                            <span className="font-semibold text-green-400">{entry.winnerName}</span>
                            {' '}besiegt{' '}
                            <span className="text-muted">{entry.loserName}</span>
                          </p>
                        </div>
                        <div className="text-sm text-muted">
                          {entry.playedAt
                            ? new Date(entry.playedAt).toLocaleString('de-DE', { day: '2-digit', month: '2-digit', year: 'numeric', hour: '2-digit', minute: '2-digit' })
                            : 'Zeitpunkt nicht verfügbar'}
                        </div>
                      </div>
                    </Card>
                  ))}
                </div>
              </>
            )}
          </div>
        )}
      </div>

      {/* Confirmation Modal – Team wechseln */}
      {pendingJoinTeam && userTeam && (
        <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-50">
          <div className="bg-card border border-border rounded-xl p-6 max-w-sm w-full mx-4 shadow-xl">
            <h3 className="text-base font-semibold text-foreground mb-3">Team wechseln?</h3>
            <p className="text-sm text-muted mb-5">
              Du bist bereits in Team <span className="text-foreground font-medium">"{userTeam.name}"</span>.
              Wenn du <span className="text-foreground font-medium">"{pendingJoinTeam.name}"</span> beitrittst, verlässt du dein aktuelles Team automatisch.
            </p>
            <div className="flex gap-2 justify-end">
              <Button variant="ghost" size="sm" onClick={() => setPendingJoinTeam(null)} disabled={joinTeamMutation.isPending}>Abbrechen</Button>
              <Button variant="primary" size="sm" disabled={joinTeamMutation.isPending}
                onClick={() => joinTeamMutation.mutate({ tournamentId, teamId: pendingJoinTeam.id }, { onSettled: () => setPendingJoinTeam(null) })}>
                {joinTeamMutation.isPending ? 'Wechsle...' : 'Team wechseln'}
              </Button>
            </div>
          </div>
        </div>
      )}
    </div>
  )
}
