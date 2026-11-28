import { Fragment, useState, useEffect } from 'react'
import { pluralize, registrationSummary } from '@/lib/countLabel'
import { useMutation, useQueryClient } from '@tanstack/react-query'
import { loginGuest } from '@/api/client'
import type { FormEvent } from 'react'
import { Link, useParams, useSearchParams } from 'react-router-dom'
import { motion } from 'framer-motion'
import {
  useTournament, useMyTournamentStatus, useCreateTeam, useJoinTeam, useSignupSolo,
  useWithdrawSolo, useLeaveTeam, useCheckin, useCheckinStatus,
  useMyInvitations, useAcceptInvitation, useRejectInvitation, useApplyToTeam,
  useConsent, useSetMemberLineup,
} from '@/hooks/useTournament'
import { benchFreeLabel, lineupOf, starterCount, teamFormatLabel } from '@/lib/lineup'
import { useAuth } from '@/hooks/useAuth'
import { latestActionError } from '@/lib/latestError'
import Card from '@/components/ui/Card'
import Badge from '@/components/ui/Badge'
import Button from '@/components/ui/Button'
import LoadingSpinner from '@/components/ui/LoadingSpinner'
import ConsentModal from '@/components/ConsentModal'
import { LEAGUE_LABEL, isLeague, plannedTournamentStart } from '@/lib/league'
import { TOURNAMENT_RULES_URL } from '@/lib/discord'
import GroupStandings from '@/components/groups/GroupStandings'
import GroupMatchList from '@/components/groups/GroupMatchList'
import BracketView from '@/components/bracket/BracketView'
import MiniGroupPanel from '@/components/bracket/MiniGroupPanel'
import MyMatchReport from '@/components/MyMatchReport'
import LoginButton from '@/components/auth/LoginButton'
import LiveNow from '@/components/tournament/LiveNow'
import TournamentDescription from '@/components/tournament/TournamentDescription'
import { whenLabel } from '@/lib/whenLabel'
import {
  Trophy, Users, LayoutGrid, GitBranch, Shield, X, Info, Book,
  ScrollText, CheckCircle2, ClipboardCheck, Mail, BarChart2, User,
} from 'lucide-react'
import type { TeamPublic, BracketMatch, GroupMatch, TournamentDetailPublic, TournamentGameMode } from '@/types/tournament'

const GAME_MODE_LABEL: Record<TournamentGameMode, string> = {
  standard: 'Standard',
  mirror: 'Mirror Match',
  all_same: 'All Same Hero',
  random_heroes: 'Random Heroes',
  single_lane: 'Single Lane Battle',
}
import { ApiError } from '@/api/client'
import { bracketFormatLabel, effectiveBracketFormat } from '@/lib/bracketFormat'
import { bracketPlacements } from '@/lib/bracketPlacements'
import { bracketStageLabel } from '@/lib/bracketStage'
import ReactMarkdown from 'react-markdown'
import { playerName } from '@/lib/playerName'
import { teamLabel, teamNameById } from '@/lib/teamName'

type Tab = 'übersicht' | 'kodex' | 'gruppen' | 'bracket' | 'teams' | 'ergebnisse' | 'rangliste'

const TAB_KEYS: Tab[] = ['übersicht', 'kodex', 'gruppen', 'bracket', 'teams', 'ergebnisse', 'rangliste']

function isTab(value: string | null): value is Tab {
  return value !== null && (TAB_KEYS as string[]).includes(value)
}

function defaultTab(status: string | undefined): Tab {
  if (status === 'bracket') return 'bracket'
  if (status === 'group_phase') return 'gruppen'
  return 'übersicht'
}

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
  matchId: number
}

function getTeamName(teamId: number | null, teams: TeamPublic[]): string {
  if (teamId === null) return 'Freilos'
  return teamNameById(teams, teamId)
}

interface BracketRankEntry {
  position: number
  teamId: number
  teamName: string
  label: string
}

function deriveBracketPlacements(matches: BracketMatch[], teams: TeamPublic[]): BracketRankEntry[] {
  return bracketPlacements(matches).map((entry) => ({ ...entry, teamName: getTeamName(entry.teamId, teams) }))
}

function playedTime(entry: ResultEntry): number {
  const parsed = entry.playedAt ? Date.parse(entry.playedAt) : Number.NaN
  return Number.isNaN(parsed) ? 0 : parsed
}

function getSortTime(value: string | null, fallback: number): number {
  if (!value) return Number.MAX_SAFE_INTEGER - fallback
  const parsed = new Date(value).getTime()
  return Number.isNaN(parsed) ? Number.MAX_SAFE_INTEGER - fallback : parsed
}

function tournamentChampion(tournament: TournamentDetailPublic): string | null {
  if (!['completed', 'archived'].includes(tournament.status)) return null
  if (isLeague(tournament.tournament_mode)) {
    const leader = tournament.groups[0]?.teams[0]
    return leader ? leader.team_name : null
  }
  const winner = bracketPlacements(tournament.bracket_matches).find((entry) => entry.position === 1)
  return winner ? getTeamName(winner.teamId, tournament.teams) : null
}

function isCompletedMatch(status: string): boolean {
  return status === 'completed' || status === 'forfeit'
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
        title: match.round ? `Runde ${match.round}` : `${group.name} • Runde ${index + 1}`,
        winnerName,
        loserName: getTeamName(loserId, teams),
        playedAt: match.played_at,
        matchId: match.id,
      }
    })
  })
}

function buildBracketResultEntries(matches: BracketMatch[], teams: TeamPublic[]): ResultEntry[] {
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
        title: bracketStageLabel(match, matches),
        winnerName,
        loserName: getTeamName(loserId, teams),
        playedAt: match.played_at,
        matchId: match.id,
      }
    })
}

function recruitingLabel(status: string, full: boolean) {
  if (full) return { text: 'Voll', color: 'text-muted bg-border/30 border-border' }
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
  const queryClient = useQueryClient()
  const [guestName, setGuestName] = useState('')
  const guestLogin = useMutation({
    mutationFn: loginGuest,
    onError: (error) => {
      if (error instanceof ApiError && error.status === 409) void queryClient.invalidateQueries({ queryKey: ['auth', 'me'] })
    },
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: ['auth', 'me'] })
      await queryClient.invalidateQueries({ queryKey: ['consent'] })
      setPendingAction(() => () => signupSoloMutation.mutate(tournamentId, {
        onSuccess: () => setSuccessMsg('Du bist jetzt als Solo-Spieler eingetragen!'),
      }))
      setShowConsentModal(true)
    },
  })
  const [searchParams, setSearchParams] = useSearchParams()
  const [showCreateTeam, setShowCreateTeam] = useState(false)
  const [teamName, setTeamName] = useState('')
  const [successMsg, setSuccessMsg] = useState<string | null>(null)
  const [pendingAction, setPendingAction] = useState<(() => void) | null>(null)
  const [showConsentModal, setShowConsentModal] = useState(false)
  const [pendingJoinTeam, setPendingJoinTeam] = useState<TeamPublic | null>(null)
  const [pendingJoinBench, setPendingJoinBench] = useState(false)

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
  const lineupMutation = useSetMemberLineup(tournamentId)

  useEffect(() => {
    if (!successMsg) return
    const timer = setTimeout(() => setSuccessMsg(null), 3000)
    return () => clearTimeout(timer)
  }, [successMsg])

  const league = isLeague(tournament?.tournament_mode)
  const showResultsTab = tournament
    ? ['group_phase', 'bracket', 'completed', 'archived'].includes(tournament.status)
    : false
  const bracketOnly = tournament?.tournament_mode === 'bracket_only'
  const availableTabs = ALL_TABS.filter(
    (tab) => showResultsTab || (tab.key !== 'ergebnisse' && tab.key !== 'rangliste')
  )
    .filter((tab) => !league || tab.key !== 'bracket')
    .filter((tab) => !bracketOnly || tab.key !== 'gruppen')
    .map((tab) => (league && tab.key === 'gruppen' ? { ...tab, label: 'Tabelle' } : tab))
  const tabParam = searchParams.get('tab')
  const selectedTab = isTab(tabParam) ? tabParam : defaultTab(tournament?.status)
  const activeTab = availableTabs.some((tab) => tab.key === selectedTab)
    ? selectedTab
    : 'übersicht'
  const setActiveTab = (tab: Tab) => {
    setSearchParams((params) => {
      const next = new URLSearchParams(params)
      next.set('tab', tab)
      return next
    }, { replace: true })
  }

  if (isLoading) return <LoadingSpinner />
  if (!tournament) {
    return (
      <Card className="space-y-4 py-20 text-center">
        <p className="text-muted">Dieses Turnier wurde nicht gefunden.</p>
        <Link to="/" className="inline-block text-sm font-semibold text-primary underline-offset-4 hover:underline">
          Zurück zu den Turnieren
        </Link>
      </Card>
    )
  }

  const isRegistration = tournament.status === 'registration'
  const soloSignups = tournament.signups.filter((signup) => signup.team_id === null)
  const tournamentStart = plannedTournamentStart(tournament)
  const champion = tournamentChampion(tournament)
  const isSignupOpen = tournament.registration_open
  const isCheckinPhase = tournament.status === 'checkin'
  const soloOnly = tournament.invite_mode === 'solo_only'
  const formatLabel = bracketFormatLabel(effectiveBracketFormat(tournament))
  const registrationLabel = registrationSummary(
    tournament.teams.length,
    tournament.signups.filter((signup) => signup.team_id === null).length,
  )

  const userTeam = myStatus?.team_id != null
    ? tournament.teams.find((t) => t.id === myStatus.team_id) ?? null
    : null
  const userSignupId = myStatus?.signup_id ?? null
  const userHasSoloSignup = userSignupId !== null && !myStatus?.team_id
  const isUserCaptain = myStatus?.is_captain ?? false
  const benchSlots = tournament.bench_slots ?? 0
  const canEditLineup = isUserCaptain && isRegistration && benchSlots > 0 && !soloOnly
  const userStartersFull = userTeam != null && starterCount(userTeam.members) >= tournament.team_size
  const userBenchFull = userTeam != null && userTeam.members.length - starterCount(userTeam.members) >= benchSlots
  const isUserRegistered = Boolean(userTeam || userHasSoloSignup)
  const hasCheckedIn = myStatus?.is_checked_in ?? false

  const resultEntries = tournament
    ? [
        ...buildGroupResultEntries(tournament.groups, tournament.teams),
        ...buildBracketResultEntries(tournament.bracket_matches, tournament.teams),
      ].sort((left, right) => playedTime(right) - playedTime(left) || right.matchId - left.matchId)
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
    applyMutation.isPending ||
    lineupMutation.isPending

  const latestError = latestActionError([
    createTeamMutation,
    joinTeamMutation,
    signupSoloMutation,
    withdrawSoloMutation,
    leaveTeamMutation,
    checkinMutation,
    acceptInviteMutation,
    rejectInviteMutation,
    applyMutation,
    lineupMutation,
  ])
  const actionErrors =
    latestError instanceof Error && latestError.message !== 'CONSENT_REQUIRED' ? [latestError.message] : []

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

  const handleJoinTeam = (team: TeamPublic, asBench = false) => {
    withConsent(() => {
      if (userTeam) {
        setPendingJoinBench(asBench)
        setPendingJoinTeam(team)
      } else {
        joinTeamMutation.mutate({ tournamentId, teamId: team.id, asBench }, { onError: handleConsentError })
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

  const handleApply = (teamId: number, asBench = false) => {
    withConsent(() => {
      applyMutation.mutate({ teamId, asBench }, {
        onSuccess: () => setSuccessMsg('Bewerbung wurde gesendet.'),
        onError: handleConsentError,
      })
    })
  }

  return (
    <motion.div
      initial={{ opacity: 0 }}
      animate={{ opacity: 1 }}
      className="space-y-6 pb-20 sm:space-y-8"
    >
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

      <section className="relative overflow-hidden rounded-lg border-b border-white/5 pb-6 sm:pb-12">
        <div className="absolute top-0 right-0 p-4">
          <Badge status={tournament.status} mode={tournament.tournament_mode} className="scale-110" />
        </div>
        <div className="space-y-4">
          <div className="flex items-center gap-3">
             <div className="p-2 rounded-lg bg-primary/10 border border-primary/20">
                <Trophy size={20} className="text-primary" />
             </div>
             <p className="text-xs font-semibold text-primary uppercase tracking-[0.12em]">Turnier</p>
          </div>
          <h1 className="text-3xl sm:text-4xl md:text-5xl font-bold text-foreground font-display tracking-tight">{tournament.name}</h1>
          {tournament.description && (
            <TournamentDescription text={tournament.description} />
          )}
          {champion && (
            <p className="flex items-center gap-2 text-lg font-bold text-primary">
              <Trophy size={18} className="shrink-0" />
              Turniersieger: {champion}
            </p>
          )}

          <div className="flex flex-wrap items-center gap-x-8 gap-y-3 pt-2 sm:pt-4">
            <div className="space-y-1">
              <p className="text-xs uppercase font-bold text-muted tracking-widest">Team-Format</p>
              <p className="text-sm font-bold text-foreground uppercase">{teamFormatLabel(tournament.team_size, tournament.bench_slots ?? 0)}</p>
            </div>
            <div className="space-y-1">
              <p className="text-xs uppercase font-bold text-muted tracking-widest">Anmeldungen</p>
              <p className="text-sm font-bold text-foreground uppercase">{registrationLabel}</p>
            </div>
            <div className="space-y-1">
              <p className="text-xs uppercase font-bold text-muted tracking-widest">Format</p>
              <p className="text-sm font-bold text-foreground uppercase">{league ? LEAGUE_LABEL : formatLabel}</p>
            </div>
            {tournament.tournament_game_mode !== 'standard' && (
              <div className="space-y-1">
                <p className="text-xs uppercase font-bold text-primary tracking-widest">Modus</p>
                <p className="text-sm font-bold text-primary uppercase">{GAME_MODE_LABEL[tournament.tournament_game_mode]}</p>
              </div>
            )}
          </div>
        </div>
      </section>

      <div className="grid grid-cols-1 gap-4 empty:hidden md:grid-cols-2">
        {actionErrors.map((message, index) => (
          <p key={`${index}-${message}`} role="alert" className="md:col-span-2 rounded-lg border border-red-500/20 bg-red-500/10 p-4 text-sm text-red-400">
            {message}
          </p>
        ))}
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

        {isCheckinPhase && (
          <Card className={`p-6 border-amber-500/20 bg-amber-500/5 ${!hasCheckedIn ? 'animate-pulse' : ''}`}>
            <div className="flex flex-col gap-4">
              <div className="space-y-1">
                <div className="flex items-center gap-2 text-amber-500">
                  <ClipboardCheck size={18} />
                  <span className="text-xs font-bold uppercase tracking-widest">Anwesenheit</span>
                </div>
                <h3 className="text-lg font-bold text-foreground uppercase font-display">Check-in aktiv</h3>
                <p className="text-xs text-muted italic">
                  Bestätige deine Teilnahme, sonst wirst du aus dem Turnier entfernt.
                </p>
              </div>
              <div className="flex items-center justify-between">
                <div className="text-xs text-muted">
                  <span className="text-foreground font-bold">{checkinStatus?.total_checked_in ?? 0}</span> / {checkinStatus?.total_registered ?? tournament.signups.length} eingecheckt
                </div>
                {isLoggedIn && isUserRegistered && (
                  <Button
                    variant={hasCheckedIn ? 'secondary' : 'primary'}
                    size="sm"
                    disabled={hasCheckedIn || checkinMutation.isPending}
                    onClick={handleCheckin}
                  >
                    {hasCheckedIn ? 'Eingecheckt' : 'Check-in bestätigen'}
                  </Button>
                )}
              </div>
            </div>
          </Card>
        )}

        {isLoggedIn && !myStatus?.team_id && myInvitations && myInvitations.length > 0 && (
          <Card className="p-6 border-primary/20 bg-primary/5">
            <div className="space-y-4">
              <div className="flex items-center gap-2 text-primary">
                <Mail size={18} />
                <h3 className="text-xs font-bold uppercase tracking-widest">Einladungen</h3>
              </div>
              <div className="space-y-3">
                {myInvitations.map((invite) => (
                  <div key={invite.id} className="flex items-center justify-between gap-3 bg-white/5 p-3 rounded-lg border border-white/5">
                    <div>
                      <p className="text-sm font-bold text-foreground uppercase tracking-wide">{teamLabel(invite.team_name, invite.team_id)}</p>
                      {invite.expires_at && (
                        <p className="text-xs text-muted italic">
                          Ablauf: {new Date(invite.expires_at).toLocaleTimeString('de-DE', { hour: '2-digit', minute: '2-digit' })}
                        </p>
                      )}
                      {soloOnly && (
                        <p className="text-xs text-muted">In diesem Turnier gibt es nur Solo-Anmeldungen, Einladungen können nicht angenommen werden.</p>
                      )}
                    </div>
                    <div className="flex gap-2">
                      {!soloOnly && (
                        <Button
                          variant="primary"
                          size="sm"
                          disabled={isMutating}
                          onClick={() => acceptInviteMutation.mutate(invite.id, { onSuccess: () => setSuccessMsg('Team beigetreten!') })}
                        >
                          Annehmen
                        </Button>
                      )}
                      <button
                        className="p-2 text-muted hover:text-danger transition-colors disabled:opacity-50"
                        disabled={isMutating}
                        aria-label="Einladung ablehnen"
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

      <LiveNow
        tournament={tournament}
        start={tournamentStart}
        canOpen={(phase) => phase !== activeTab && availableTabs.some((tab) => tab.key === phase)}
        onOpen={setActiveTab}
      />

      <div className="relative">
      <div role="tablist" aria-label="Turnierbereiche" className="flex gap-1 overflow-x-auto border-b border-white/5 pb-px pr-8 sm:pr-0">
        {availableTabs.map((tab) => {
          const Icon = tab.icon
          const isActive = activeTab === tab.key
          return (
            <button
              key={tab.key}
              type="button"
              role="tab"
              aria-selected={isActive}
              aria-controls="turnier-tabpanel"
              id={`turnier-tab-${tab.key}`}
              onClick={() => setActiveTab(tab.key)}
              className={`relative flex shrink-0 items-center gap-2 whitespace-nowrap px-3 py-3 text-[13px] font-semibold transition-colors sm:px-5 sm:py-4 ${
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
      <div aria-hidden="true" className="pointer-events-none absolute inset-y-0 right-0 w-10 bg-gradient-to-l from-background to-transparent sm:hidden" />
      </div>

      <motion.div
        key={activeTab}
        id="turnier-tabpanel"
        role="tabpanel"
        aria-labelledby={`turnier-tab-${activeTab}`}
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
                    Zeitplan
                  </h2>
                  <div className="grid grid-cols-1 sm:grid-cols-2 gap-8">
                    {tournament.registration_start && (
                      <div className="space-y-2">
                        <p className="text-xs uppercase font-bold text-muted tracking-widest">Anmeldung ab</p>
                        <p className="text-lg font-bold text-foreground">
                          {whenLabel(tournament.registration_start)}
                        </p>
                      </div>
                    )}
                    {tournament.checkin_start && (
                      <div className="space-y-2">
                        <p className="text-xs uppercase font-bold text-muted tracking-widest">Check-in ab</p>
                        <p className="text-lg font-bold text-foreground">
                          {whenLabel(tournament.checkin_start)}
                        </p>
                      </div>
                    )}
                    {tournamentStart && (
                      <div className="space-y-2">
                        <p className="text-xs uppercase font-bold text-primary tracking-widest">Turnierbeginn</p>
                        <p className="text-lg font-bold text-primary">
                          {whenLabel(tournamentStart)}
                        </p>
                      </div>
                    )}
                  </div>
                </Card>

                {isSignupOpen && (
                   <Card className="p-8 border-primary/10 bg-primary/5 space-y-6">
                     <div className="flex items-center justify-between">
                        <h2 className="text-xl font-bold font-display uppercase tracking-widest text-foreground">Deine Anmeldung</h2>
                        <Button variant="ghost" size="sm" onClick={() => setActiveTab('teams')}>
                          Zu den Teams <Users size={14} />
                        </Button>
                     </div>

                     {!isLoggedIn ? (
                        <div className="text-center py-6 space-y-4">
                           <LoginButton />
                           <form className="mx-auto max-w-sm space-y-3 text-left" onSubmit={(event) => {
                             event.preventDefault()
                             if (guestName.trim()) guestLogin.mutate(guestName.trim())
                           }}>
                             <p className="text-sm text-muted">Kein Discord? Melde dich mit deinem Namen an.</p>
                             <label htmlFor="guest-name" className="block text-sm text-foreground">Dein Spielername</label>
                             <input id="guest-name" name="guest-name" type="text" required
                               autoComplete="nickname" value={guestName} disabled={guestLogin.isPending}
                               onChange={(event) => setGuestName(event.target.value)}
                               className="w-full rounded-lg border border-border bg-background p-3 text-foreground" />
                             <p className="text-xs text-muted">Dein Gastzugang bleibt in diesem Browser gespeichert. Nach dem Abmelden oder Löschen der Cookies hilft dir die Turnierleitung weiter.</p>
                             <Button type="submit" variant="secondary" disabled={guestLogin.isPending || !guestName.trim()}>
                               {guestLogin.isPending ? 'Wird angemeldet…' : 'Mit Namen teilnehmen'}
                             </Button>
                             {guestLogin.isError && <p role="alert" className="text-sm text-red-400">{guestLogin.error.message}</p>}
                           </form>
                        </div>
                     ) : !userTeam && !userHasSoloSignup ? (
                        <div className="space-y-4">
                          <p className="text-muted italic text-sm">
                            {soloOnly
                              ? 'Melde dich einzeln an. Das Orga-Team stellt die Teams zusammen.'
                              : 'Melde dich allein an oder gründe ein Team.'}
                          </p>
                          <div className="flex flex-wrap gap-4">
                            <Button variant="primary" onClick={handleSignupSolo} disabled={signupSoloMutation.isPending}>
                              Solo anmelden
                            </Button>
                            {!soloOnly && (
                              <Button variant="secondary" onClick={() => setActiveTab('teams')}>
                                Team gründen
                              </Button>
                            )}
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
                                <p className="text-xs uppercase font-bold tracking-widest opacity-60 leading-none mb-1">Eingetragen mit</p>
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
                  <h3 className="text-xs font-bold uppercase tracking-widest text-muted mb-4 border-b border-white/5 pb-2">Hinweise</h3>
                  <div className="space-y-4">
                    {soloOnly && (
                       <div className="space-y-1">
                          <p className="text-xs uppercase font-bold text-primary tracking-widest">PUG</p>
                          <p className="text-xs text-foreground italic">Nur Solo-Anmeldung. Die Teams stellt das Orga-Team zusammen.</p>
                       </div>
                    )}
                    {tournament.invite_mode === 'window' && (
                       <div className="space-y-1">
                          <p className="text-xs uppercase font-bold text-amber-500 tracking-widest">Einladungsfenster</p>
                          <p className="text-xs text-foreground italic">Nur zu bestimmten Zeiten offen.</p>
                       </div>
                    )}
                    <div className="space-y-1">
                       <p className="text-xs uppercase font-bold text-muted tracking-widest">Fairplay</p>
                       <p className="text-xs text-foreground italic">Unsportlichkeit führt zum Ausschluss.</p>
                    </div>
                    {tournament.rules && (
                       <div className="pt-4 border-t border-white/5">
                          <button
                            onClick={() => setActiveTab('kodex')}
                            className="flex items-center gap-2 text-left text-sm font-semibold text-primary hover:text-primary-hover transition-colors"
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
               <h2 className="text-2xl font-bold tracking-widest text-foreground uppercase font-display">Regelwerk</h2>
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
                       <p className="text-xl">Für dieses Turnier gibt es keine eigenen Regeln.</p>
                       <p className="text-sm mt-2">Es gelten die <a href={TOURNAMENT_RULES_URL} target="_blank" rel="noopener noreferrer" className="text-primary underline">Turnierregeln auf Discord</a>.</p>
                    </div>
                  )}
               </div>
            </Card>
          </div>
        )}

        {activeTab === 'teams' && (
          <div className="space-y-8">
            <div className={`grid min-w-0 gap-6 ${userTeam || (isSignupOpen && !soloOnly && isLoggedIn && !userHasSoloSignup) ? 'xl:grid-cols-[minmax(14rem,260px)_minmax(0,1fr)]' : ''}`}>
              {(userTeam || (isSignupOpen && !soloOnly && isLoggedIn && !userHasSoloSignup)) && <div className="space-y-6">
                 {isSignupOpen && !soloOnly && isLoggedIn && !userTeam && !userHasSoloSignup && (
                    <Card className="p-6 space-y-6 border-primary/10">
                      <h3 className="text-sm font-bold uppercase tracking-widest text-foreground">Team gründen</h3>
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
                              placeholder="Teamname..."
                              className="w-full bg-white/5 border border-white/10 rounded-lg px-4 py-3 text-sm focus:outline-none focus:border-primary/50"
                            />
                            <div className="flex gap-2">
                              <Button type="submit" size="sm" className="flex-1" disabled={createTeamMutation.isPending}>
                                Gründen
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
                        <p className="text-[10px] font-bold text-primary uppercase tracking-widest mb-1">Dein Team</p>
                        <h3 className="text-xl font-bold uppercase tracking-tight text-foreground">{userTeam.name}</h3>
                      </div>

                      <div className="space-y-3">
                        {userTeam.members.map((m, i) => (
                          <div key={m.id ?? i} className="flex items-center justify-between gap-2 text-xs border-b border-white/5 pb-2 last:border-0">
                            <span className="font-bold text-foreground uppercase tracking-wide">{playerName(m.discord_name)}</span>
                            <span className="flex shrink-0 items-center gap-2">
                              {m.role === 'captain' && <span className="text-[10px] text-primary font-bold uppercase">Captain</span>}
                              {m.is_bench && <span className="text-[10px] text-muted font-bold uppercase">Ersatz</span>}
                              {canEditLineup && m.role !== 'captain' && m.id != null && (m.is_bench ? !userStartersFull : !userBenchFull) && (
                                <button
                                  type="button"
                                  className="rounded border border-white/10 px-2 py-0.5 text-[10px] font-bold uppercase text-muted hover:text-foreground disabled:opacity-50"
                                  disabled={isMutating}
                                  onClick={() => lineupMutation.mutate({ teamId: userTeam.id, memberId: m.id as number, isBench: !m.is_bench })}
                                >
                                  {m.is_bench ? 'In die Startaufstellung' : 'Auf die Ersatzbank'}
                                </button>
                              )}
                              {canEditLineup && m.is_bench && userStartersFull && m.id != null && (
                                <select
                                  value=""
                                  disabled={isMutating}
                                  aria-label={`${playerName(m.discord_name)} einwechseln`}
                                  className="rounded border border-white/10 bg-card px-2 py-0.5 text-[10px] font-bold uppercase text-muted"
                                  onChange={(event) => {
                                    const partner = Number(event.target.value)
                                    if (partner) lineupMutation.mutate({ teamId: userTeam.id, memberId: m.id as number, isBench: false, swapMemberId: partner })
                                  }}
                                >
                                  <option value="">Tauschen mit …</option>
                                  {userTeam.members.filter((s) => !s.is_bench && s.role !== 'captain' && s.id != null).map((s) => (
                                    <option key={s.id} value={s.id as number}>{playerName(s.discord_name)}</option>
                                  ))}
                                </select>
                              )}
                            </span>
                          </div>
                        ))}
                      </div>

                      {isSignupOpen && soloOnly && (
                        <p className="text-sm text-muted">Zum Austragen wende dich bitte an das Orga-Team.</p>
                      )}
                      {isRegistration && !soloOnly && (
                         <Button
                          variant="ghost"
                          size="sm"
                          className="w-full text-red-400 border border-red-500/20 hover:bg-red-500/10"
                          onClick={() => handleLeaveTeam(userTeam.id)}
                          disabled={isMutating || (isUserCaptain && userTeam.members.length > 1)}
                        >
                          Team verlassen
                         </Button>
                      )}
                    </Card>
                 )}
              </div>}

              <div className="min-w-0 space-y-6">
                <div className="flex items-center justify-between">
                  <h3 className="text-sm font-bold uppercase tracking-widest text-muted">Gemeldete Teams</h3>
                  <span className="text-xs font-bold text-foreground bg-white/5 px-2 py-1 rounded-lg border border-white/5">
                    {tournament.teams.length}
                  </span>
                </div>

                <div className="grid grid-cols-1 gap-4 lg:grid-cols-2">
                  {tournament.teams.map((team) => {
                    const lineup = lineupOf(team.members, tournament.team_size, benchSlots)
                    const teamFull = tournament.team_size > 0 && lineup.starterFree === 0
                    const benchOpen = benchSlots > 0 && lineup.benchFree > 0
                    const recruiting = recruitingLabel(team.recruitment_status, teamFull && !benchOpen)
                    const isMyTeam = userTeam?.id === team.id
                    return (
                      <Card key={team.id} className={`p-5 transition-all group ${isMyTeam ? 'border-primary/50' : 'hover:border-white/20'}`}>
                        <div className="space-y-4">
                          <div className="flex items-start justify-between gap-3">
                            <div>
                              <h4 className="font-bold text-foreground uppercase tracking-wide group-hover:text-primary transition-colors">{team.name}</h4>
                              <p className="text-[10px] text-muted font-bold uppercase mt-1">
                                {lineup.starters} / {tournament.team_size} {benchSlots > 0 ? 'Stammspieler' : 'Mitglieder'}
                              </p>
                            </div>
                            {!soloOnly && (
                              <span className={`text-[10px] px-2 py-0.5 rounded-lg border font-bold uppercase tracking-widest ${recruiting.color}`}>
                                {recruiting.text}
                              </span>
                            )}
                          </div>

                          <ul className="space-y-1.5 border-t border-border/60 pt-3" aria-label={`Mitglieder von ${team.name}`}>
                            {team.members.length === 0 && (
                              <li className="text-xs text-muted">Noch keine Spieler im Team.</li>
                            )}
                            {[...team.members.filter((m) => !m.is_bench), ...team.members.filter((m) => m.is_bench)].map((m, i, seats) => (
                              <Fragment key={m.id ?? `${team.id}-${i}`}>
                              {m.is_bench && !seats[i - 1]?.is_bench && (
                                <li className="px-3 pt-2 text-[10px] font-bold uppercase tracking-widest text-muted">Ersatzbank</li>
                              )}
                              <li className="flex min-w-0 items-center justify-between gap-3 rounded-md bg-white/5 px-3 py-2">
                                <span className="flex min-w-0 items-center gap-2">
                                  <User size={14} className="shrink-0 text-muted" />
                                  <span className="min-w-0 break-words text-sm font-medium text-foreground">
                                    {playerName(m.discord_name)}
                                  </span>
                                </span>
                                {m.role === 'captain' && (
                                  <span className="shrink-0 text-[10px] font-bold uppercase text-primary">Captain</span>
                                )}
                                {m.is_bench && (
                                  <span className="shrink-0 text-[10px] font-bold uppercase text-muted">Ersatz</span>
                                )}
                              </li>
                              </Fragment>
                            ))}
                            {lineup.starterFree > 0 && (
                              <li className="px-3 pt-1 text-xs text-muted">
                                {pluralize(lineup.starterFree, 'Platz', 'Plätze')} frei
                              </li>
                            )}
                            {benchOpen && (
                              <li className="px-3 pt-1 text-xs text-muted">{benchFreeLabel(lineup.benchFree)}</li>
                            )}
                          </ul>

                          {!isMyTeam && isSignupOpen && !soloOnly && (!teamFull || benchOpen) && (
                            <div className="flex flex-col gap-2 pt-2 sm:flex-row">
                               {team.recruitment_status === 'open' ? (
                                 <>
                                   {!teamFull && (
                                     <Button variant="secondary" size="sm" className="w-full" disabled={isMutating} onClick={() => handleJoinTeam(team)}>
                                       Beitreten
                                     </Button>
                                   )}
                                   {benchOpen && (
                                     <Button variant="ghost" size="sm" className="w-full border border-white/10" disabled={isMutating} onClick={() => handleJoinTeam(team, true)}>
                                       Als Ersatz beitreten
                                     </Button>
                                   )}
                                 </>
                               ) : team.recruitment_status === 'application' ? (
                                 <>
                                   {!teamFull && (
                                     <Button variant="outline" size="sm" className="w-full border-amber-500/30 text-amber-500 hover:bg-amber-500/10" disabled={isMutating} onClick={() => handleApply(team.id)}>
                                       Bewerben
                                     </Button>
                                   )}
                                   {benchOpen && (
                                     <Button variant="ghost" size="sm" className="w-full border border-amber-500/20 text-amber-500" disabled={isMutating} onClick={() => handleApply(team.id, true)}>
                                       Als Ersatz bewerben
                                     </Button>
                                   )}
                                 </>
                               ) : null}
                            </div>
                          )}
                        </div>
                      </Card>
                    )
                  })}
                </div>
                {tournament.teams.length === 0 && (
                  <Card className="p-6 text-sm text-muted">
                    Noch keine Teams eingetragen. Die Turnierleitung stellt die Teams zusammen.
                  </Card>
                )}
              </div>
            </div>

            {(isRegistration || isCheckinPhase) && <section aria-label="Solo-Spieler ohne Team" className="space-y-3 border-t border-border pt-6">
              <div className="flex flex-wrap items-center justify-between gap-3">
                <div>
                  <h3 className="text-base font-bold text-foreground">Noch nicht zugeteilte Solo-Spieler</h3>
                  <p className="mt-1 text-sm text-muted">
                    Diese Spieler haben sich angemeldet, sind aber noch keinem Team zugeordnet.
                  </p>
                </div>
                <span className="rounded-lg border border-border bg-card px-3 py-1 text-sm font-semibold text-foreground">
                  {soloSignups.length}
                </span>
              </div>
              {soloSignups.length === 0 ? (
                <p className="rounded-xl border border-border bg-card/40 px-4 py-5 text-sm text-muted">
                  Momentan keine offenen Solo-Anmeldungen.
                </p>
              ) : (
                <ul className="grid gap-2 sm:grid-cols-2 lg:grid-cols-3">
                  {soloSignups.map((signup) => (
                    <li
                      key={signup.id}
                      className="flex min-w-0 items-center gap-2 rounded-lg border border-border bg-card/60 px-3 py-3"
                    >
                      <User size={15} className="shrink-0 text-muted" />
                      <span className="min-w-0 break-words text-sm font-medium text-foreground">
                        {playerName(signup.discord_name)}
                      </span>
                    </li>
                  ))}
                </ul>
              )}
            </section>}
          </div>
        )}

        {activeTab === 'gruppen' && (
          <div className="space-y-12">
            <GroupStandings groups={tournament.groups} teams={tournament.teams} league={league} />
            <div className="border-t border-white/5 pt-12">
               <GroupMatchList groups={tournament.groups} teams={tournament.teams} league={league} />
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
            <BracketView matches={tournament.bracket_matches} teams={tournament.teams} format={effectiveBracketFormat(tournament)} miniGroups={tournament.mini_groups} />
            {tournament.mini_groups.length > 0 && (
              <MiniGroupPanel
                miniGroups={tournament.mini_groups}
                matches={tournament.bracket_matches}
                teams={tournament.teams}
              />
            )}
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
                <p className="text-xl">Noch keine Ergebnisse.</p>
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
                          <p className="text-xs uppercase font-bold text-primary tracking-widest">{entry.title}</p>
                          <div className="flex flex-wrap items-center gap-x-2 text-foreground font-bold">
                            <span className="text-primary">{entry.winnerName}</span>
                            <span className="text-muted font-normal text-xs uppercase">Besiegt</span>
                            <span className="text-muted">{entry.loserName}</span>
                          </div>
                        </div>
                        <div className="text-xs text-muted font-semibold">
                          {entry.playedAt
                            ? new Date(entry.playedAt).toLocaleString('de-DE', { weekday: 'short', day: '2-digit', month: '2-digit', year: 'numeric', hour: '2-digit', minute: '2-digit' })
                            : 'Zeit unbekannt'}
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

      {pendingJoinTeam && userTeam && (
        <div className="fixed inset-0 bg-background/90 backdrop-blur-sm flex items-center justify-center z-[100]">
          <motion.div
            initial={{ scale: 0.9, opacity: 0 }}
            animate={{ scale: 1, opacity: 1 }}
            className="glass-card p-8 max-w-sm w-full mx-4 shadow-2xl border-primary/20"
          >
            <h3 className="text-xl font-bold text-foreground font-display uppercase tracking-widest mb-4">Team wechseln?</h3>
            <p className="text-sm text-muted italic mb-8 leading-relaxed">
              Du bist bereits im Team <span className="text-primary font-bold">"{userTeam.name}"</span>.
              Willst du es verlassen und <span className="text-foreground font-bold">"{pendingJoinTeam.name}"</span> beitreten?
            </p>
            <div className="flex gap-4">
              <Button variant="ghost" className="flex-1" onClick={() => setPendingJoinTeam(null)} disabled={joinTeamMutation.isPending}>Bleiben</Button>
              <Button variant="primary" className="flex-1" disabled={joinTeamMutation.isPending}
                onClick={() => joinTeamMutation.mutate({ tournamentId, teamId: pendingJoinTeam.id, asBench: pendingJoinBench }, { onSettled: () => setPendingJoinTeam(null) })}>
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
  const league = isLeague(tournament.tournament_mode)
  const finished = ['completed', 'archived'].includes(tournament.status)
  const groupTeams = tournament.groups.flatMap((group) =>
    group.teams.map((team) => ({ ...team, groupName: group.name })),
  )
  const rankedTeams = league
    ? groupTeams
    : [...groupTeams].sort((a, b) => (b.points !== a.points ? b.points - a.points : b.wins - a.wins))

  if (bracketEntries.length === 0 && !hasGroups) {
    return (
      <Card className="p-20 text-center opacity-40 italic">
        <BarChart2 size={48} className="mx-auto text-muted mb-4 opacity-20" />
        <p className="text-xl">Noch keine Platzierungen.</p>
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
                  <th className="px-3 py-3 sm:px-6 sm:py-4 font-bold uppercase tracking-widest text-muted text-xs w-20 text-center">Rang</th>
                  <th className="px-3 py-3 sm:px-6 sm:py-4 font-bold uppercase tracking-widest text-muted text-xs">Team</th>
                  <th className="px-3 py-3 sm:px-6 sm:py-4 font-bold uppercase tracking-widest text-muted text-xs text-right">Resultat</th>
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
                    <td className="px-3 py-3 sm:px-6 sm:py-4 text-center">
                       {entry.position === 1 ? <Trophy size={18} className="text-primary mx-auto" /> : <span className="font-bold text-muted">{entry.position}</span>}
                    </td>
                    <td className="px-3 py-3 sm:px-6 sm:py-4 font-bold text-foreground uppercase tracking-wide">{entry.teamName}</td>
                    <td className="px-3 py-3 sm:px-6 sm:py-4 text-right">
                      <span className="whitespace-nowrap text-xs px-2 py-0.5 rounded-lg border border-primary/20 text-primary font-bold uppercase tracking-widest bg-primary/5">
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
             <h2 className="text-xl font-bold tracking-widest text-foreground uppercase">{league ? (finished ? 'Endplatzierung' : 'Aktuelle Tabelle') : 'Gruppenwertung'}</h2>
          </div>
          <Card className="p-0 overflow-hidden border-white/5">
            <table className="w-full text-sm">
              <thead>
                <tr className="bg-white/5 text-left border-b border-white/10">
                  <th className="px-3 py-3 sm:px-6 sm:py-4 font-bold uppercase tracking-widest text-muted text-xs w-20 text-center">#</th>
                  <th className="px-3 py-3 sm:px-6 sm:py-4 font-bold uppercase tracking-widest text-muted text-xs">Team</th>
                  <th className="px-3 py-3 sm:px-6 sm:py-4 font-bold uppercase tracking-widest text-muted text-xs text-center">S</th>
                  <th className="px-3 py-3 sm:px-6 sm:py-4 font-bold uppercase tracking-widest text-muted text-xs text-center">N</th>
                  <th className="px-3 py-3 sm:px-6 sm:py-4 font-bold uppercase tracking-widest text-muted text-xs text-right">Pkt</th>
                </tr>
              </thead>
              <tbody>
                {rankedTeams
                  .map((team, idx) => (
                    <tr key={`${team.team_id}-${team.groupName}`} className="border-b border-white/5 last:border-0 hover:bg-white/[0.02] transition-colors">
                      <td className="px-3 py-3 sm:px-6 sm:py-4 text-center text-muted font-bold">
                        {league && finished && idx === 0 ? <Trophy size={18} className="text-primary mx-auto" /> : idx + 1}
                      </td>
                      <td className="px-3 py-3 sm:px-6 sm:py-4 font-bold text-foreground uppercase tracking-wide">{team.team_name}</td>
                      <td className="px-3 py-3 sm:px-6 sm:py-4 text-center text-muted font-medium">{team.wins}</td>
                      <td className="px-3 py-3 sm:px-6 sm:py-4 text-center text-muted font-medium">{team.losses}</td>
                      <td className="px-3 py-3 sm:px-6 sm:py-4 text-right font-bold text-primary">{team.points}</td>
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
