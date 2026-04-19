import { useMemo, useState } from 'react'
import Card from '@/components/ui/Card'
import Button from '@/components/ui/Button'
import {
  useCreateGroupLobby,
  useFetchGroupMatchResult,
  useLeaveGroupLobby,
  useResetGroupMatch,
  useSetManualGroupMatchLobby,
  useStartGroupMatch,
  useSubmitMatchResult,
} from '@/hooks/useTournament'
import type { Group, Team } from '@/types/tournament'
import {
  AlertCircle,
  CheckCircle2,
  Copy,
  DoorOpen,
  Gavel,
  KeyRound,
  Play,
  Radio,
  RefreshCcw,
  Swords,
} from 'lucide-react'

interface GroupMatchAdminPanelProps {
  tournamentId: number
  groups: Group[]
  teams: Team[]
  onRefresh?: () => void
}

type MatchMessage = {
  kind: 'success' | 'error'
  text: string
}

type ActiveAction = {
  matchId: number
  action: 'create' | 'start' | 'fetch' | 'leave'
}

function getTeamName(teamId: number, teams: Team[], group: Group): string {
  const groupedTeam = group.teams.find((team) => team.team_id === teamId)
  if (groupedTeam) return groupedTeam.team_name
  return teams.find((team) => team.id === teamId)?.name ?? `Team #${teamId}`
}

function getStatusLabel(status: string): string {
  switch (status) {
    case 'lobby_created':
      return 'Lobby erstellt'
    case 'in_progress':
      return 'Läuft'
    case 'completed':
      return 'Abgeschlossen'
    case 'forfeit':
      return 'Forfeit'
    case 'cancelled':
      return 'Abgesagt'
    default:
      return 'Ausstehend'
  }
}

function formatSchedule(value: string | null): string | null {
  if (!value) return null
  return new Date(value).toLocaleString('de-DE', {
    day: '2-digit',
    month: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
  })
}

export default function GroupMatchAdminPanel({
  tournamentId,
  groups,
  teams,
  onRefresh,
}: GroupMatchAdminPanelProps) {
  const submitResultMutation = useSubmitMatchResult()
  const createLobbyMutation = useCreateGroupLobby()
  const startMatchMutation = useStartGroupMatch()
  const fetchResultMutation = useFetchGroupMatchResult()
  const leaveLobbyMutation = useLeaveGroupLobby()
  const resetMatchMutation = useResetGroupMatch()
  const setManualLobbyMutation = useSetManualGroupMatchLobby()

  const [selectedWinners, setSelectedWinners] = useState<Record<number, number>>({})
  const [submittingMatchId, setSubmittingMatchId] = useState<number | null>(null)
  const [activeAction, setActiveAction] = useState<ActiveAction | null>(null)
  const [messages, setMessages] = useState<Record<number, MatchMessage>>({})
  const [copyFeedback, setCopyFeedback] = useState<Record<number, boolean>>({})
  const [manualCodes, setManualCodes] = useState<Record<number, string>>({})

  const groupsWithMatches = useMemo(
    () => groups.filter((group) => group.matches.length > 0),
    [groups],
  )

  if (groupsWithMatches.length === 0) {
    return (
      <Card className="p-5">
        <div className="flex items-center gap-2 text-muted">
          <Swords size={18} />
          <span>Keine Gruppenspiele vorhanden.</span>
        </div>
      </Card>
    )
  }

  const isLocked = submittingMatchId !== null || activeAction !== null

  const setMessage = (matchId: number, message: MatchMessage) => {
    setMessages((current) => ({ ...current, [matchId]: message }))
  }

  const clearMessage = (matchId: number) => {
    setMessages((current) => {
      const next = { ...current }
      delete next[matchId]
      return next
    })
  }

  const handleSubmit = async (matchId: number) => {
    const winnerId = selectedWinners[matchId]
    if (!winnerId || isLocked) return

    setSubmittingMatchId(matchId)
    clearMessage(matchId)

    try {
      await submitResultMutation.mutateAsync({
        tournamentId,
        matchId,
        data: { winner_id: winnerId },
      })
      setMessage(matchId, { kind: 'success', text: 'Ergebnis gespeichert.' })
      setSelectedWinners((current) => {
        const next = { ...current }
        delete next[matchId]
        return next
      })
      onRefresh?.()
    } catch (error) {
      setMessage(matchId, {
        kind: 'error',
        text: error instanceof Error ? error.message : 'Ergebnis konnte nicht gespeichert werden',
      })
    } finally {
      setSubmittingMatchId(null)
    }
  }

  const handleAction = async (
    matchId: number,
    action: ActiveAction['action'],
  ) => {
    if (isLocked) return
    setActiveAction({ matchId, action })
    clearMessage(matchId)

    try {
      if (action === 'create') {
        await createLobbyMutation.mutateAsync({ tournamentId, matchId })
        setMessage(matchId, { kind: 'success', text: 'Gruppen-Lobby wurde erstellt.' })
      } else if (action === 'start') {
        await startMatchMutation.mutateAsync({ tournamentId, matchId })
        setMessage(matchId, { kind: 'success', text: 'Gruppen-Match wurde gestartet.' })
      } else if (action === 'fetch') {
        await fetchResultMutation.mutateAsync({ tournamentId, matchId })
        setMessage(matchId, { kind: 'success', text: 'Ergebnis wurde aus Deadlock übernommen.' })
      } else {
        await leaveLobbyMutation.mutateAsync({ tournamentId, matchId })
        setMessage(matchId, { kind: 'success', text: 'Bot hat die Gruppen-Lobby verlassen.' })
      }
      onRefresh?.()
    } catch (error) {
      setMessage(matchId, {
        kind: 'error',
        text: error instanceof Error ? error.message : 'Aktion konnte nicht ausgeführt werden',
      })
    } finally {
      setActiveAction(null)
    }
  }

  const handleCopyCode = async (matchId: number, partyCode: string) => {
    try {
      await navigator.clipboard.writeText(partyCode)
      setCopyFeedback((current) => ({ ...current, [matchId]: true }))
      window.setTimeout(() => {
        setCopyFeedback((current) => ({ ...current, [matchId]: false }))
      }, 1500)
    } catch {
      setMessage(matchId, { kind: 'error', text: 'Party-Code konnte nicht kopiert werden.' })
    }
  }

  const handleResetMatch = async (matchId: number) => {
    if (isLocked || !window.confirm('Gruppen-Match wirklich zurücksetzen?')) return
    setActiveAction({ matchId, action: 'leave' })
    clearMessage(matchId)
    try {
      await resetMatchMutation.mutateAsync({ tournamentId, matchId })
      setMessage(matchId, { kind: 'success', text: 'Gruppen-Match wurde zurückgesetzt.' })
      onRefresh?.()
    } catch (error) {
      setMessage(matchId, {
        kind: 'error',
        text: error instanceof Error ? error.message : 'Gruppen-Match konnte nicht zurückgesetzt werden',
      })
    } finally {
      setActiveAction(null)
    }
  }

  const handleSetManualLobby = async (matchId: number) => {
    if (isLocked) return
    const partyCode = manualCodes[matchId]?.trim()
    if (!partyCode) {
      setMessage(matchId, { kind: 'error', text: 'Bitte zuerst einen Party-Code eingeben.' })
      return
    }
    setActiveAction({ matchId, action: 'create' })
    clearMessage(matchId)
    try {
      await setManualLobbyMutation.mutateAsync({ tournamentId, matchId, partyCode })
      setMessage(matchId, { kind: 'success', text: 'Party-Code wurde manuell gespeichert.' })
      onRefresh?.()
    } catch (error) {
      setMessage(matchId, {
        kind: 'error',
        text: error instanceof Error ? error.message : 'Party-Code konnte nicht gespeichert werden',
      })
    } finally {
      setActiveAction(null)
    }
  }

  return (
    <div className="grid gap-4">
      {groupsWithMatches.map((group) => (
        <Card key={group.id} className="p-5 space-y-4">
          <div className="flex items-center justify-between gap-3">
            <div>
              <h3 className="text-lg font-semibold text-foreground">{group.name}</h3>
              <p className="mt-1 text-sm text-muted">
                Gruppen-Lobbys steuern, Ergebnisse abrufen oder manuell eintragen.
              </p>
            </div>
            <span className="text-xs text-muted">
              {group.matches.filter((match) => match.status === 'completed').length}/{group.matches.length} gespielt
            </span>
          </div>

          <div className="space-y-3">
            {group.matches.map((match) => {
              const team1Name = getTeamName(match.team1_id, teams, group)
              const team2Name = getTeamName(match.team2_id, teams, group)
              const winnerName = match.winner_id
                ? getTeamName(match.winner_id, teams, group)
                : null
              const message = messages[match.id]
              const isTerminal = ['completed', 'forfeit', 'cancelled'].includes(match.status)
              const isSubmitting = submittingMatchId === match.id
              const currentAction = activeAction?.matchId === match.id ? activeAction.action : null
              const canCreateLobby = !match.steam_party_id && ['pending', 'checkin'].includes(match.status)
              const canStartMatch = match.status === 'lobby_created' && Boolean(match.steam_party_id)
              const canFetchResult = match.status === 'in_progress' && (Boolean(match.steam_party_id) || Boolean(match.deadlock_match_id))
              const canLeaveLobby = ['lobby_created', 'in_progress'].includes(match.status) && Boolean(match.steam_party_id)
              const canReset = !['completed', 'forfeit', 'cancelled'].includes(match.status)

              return (
                <div key={match.id} className="rounded-xl border border-border bg-background/60 p-4">
                  <div className="flex flex-col gap-2 md:flex-row md:items-center md:justify-between">
                    <div>
                      <div className="flex items-center gap-2 text-sm text-muted">
                        <Gavel size={14} className="text-primary" />
                        <span>Match #{match.id}</span>
                      </div>
                      <h4 className="mt-1 text-base font-semibold text-foreground">
                        {team1Name} vs {team2Name}
                      </h4>
                    </div>
                    <div className="text-sm text-muted">
                      Status: <span className="text-foreground">{getStatusLabel(match.status)}</span>
                    </div>
                  </div>

                  <div className="mt-3 flex flex-wrap gap-3 text-xs text-muted">
                    {match.deadlock_match_id && <span>Deadlock Match-ID: {match.deadlock_match_id}</span>}
                    {match.match_duration_s !== null && <span>Dauer: {Math.floor(match.match_duration_s / 60)}m</span>}
                    {formatSchedule(match.scheduled_at) && <span>Termin: {formatSchedule(match.scheduled_at)}</span>}
                  </div>

                  {match.party_code && (
                    <div className="mt-3 rounded-xl border border-primary/20 bg-primary/10 px-4 py-3">
                      <div className="flex items-center gap-2 text-xs uppercase tracking-wider text-primary/80">
                        <KeyRound size={14} />
                        Party-Code
                      </div>
                      <div className="mt-2 text-xl font-bold tracking-[0.18em] text-foreground">
                        {match.party_code}
                      </div>
                      <Button
                        variant="secondary"
                        size="sm"
                        className="mt-3"
                        onClick={() => void handleCopyCode(match.id, match.party_code!)}
                      >
                        <Copy size={14} />
                        {copyFeedback[match.id] ? 'Kopiert' : 'Code kopieren'}
                      </Button>
                    </div>
                  )}

                  {winnerName && (
                    <p className="mt-3 text-sm text-success">
                      Gewinner: <span className="font-semibold">{winnerName}</span>
                    </p>
                  )}

                  <div className="mt-4 flex flex-wrap gap-2">
                    {canCreateLobby && (
                      <Button
                        variant="primary"
                        size="sm"
                        disabled={isLocked}
                        onClick={() => void handleAction(match.id, 'create')}
                      >
                        <Radio size={14} />
                        {currentAction === 'create' ? 'Erstellt...' : 'Lobby erstellen'}
                      </Button>
                    )}
                    {canStartMatch && (
                      <Button
                        variant="primary"
                        size="sm"
                        disabled={isLocked}
                        onClick={() => void handleAction(match.id, 'start')}
                      >
                        <Play size={14} />
                        {currentAction === 'start' ? 'Startet...' : 'Match starten'}
                      </Button>
                    )}
                    {canFetchResult && (
                      <Button
                        variant="secondary"
                        size="sm"
                        disabled={isLocked}
                        onClick={() => void handleAction(match.id, 'fetch')}
                      >
                        <RefreshCcw size={14} />
                        {currentAction === 'fetch' ? 'Lädt...' : 'Ergebnis abrufen'}
                      </Button>
                    )}
                    {canLeaveLobby && (
                      <Button
                        variant="ghost"
                        size="sm"
                        disabled={isLocked}
                        onClick={() => void handleAction(match.id, 'leave')}
                      >
                        <DoorOpen size={14} />
                        {currentAction === 'leave' ? 'Verlässt...' : 'Lobby verlassen'}
                      </Button>
                    )}
                    {canReset && (
                      <Button
                        variant="ghost"
                        size="sm"
                        disabled={isLocked}
                        onClick={() => void handleResetMatch(match.id)}
                      >
                        <RefreshCcw size={14} />
                        Match zurücksetzen
                      </Button>
                    )}
                  </div>

                  {!match.steam_party_id && !isTerminal && (
                    <div className="mt-3 grid gap-2 sm:grid-cols-[1fr_auto]">
                      <input
                        type="text"
                        value={manualCodes[match.id] ?? ''}
                        onChange={(event) => setManualCodes((current) => ({ ...current, [match.id]: event.target.value }))}
                        placeholder="Manuellen Party-Code eintragen"
                        className="w-full rounded-lg border border-border bg-background px-3 py-2 text-sm text-foreground focus:outline-none focus:ring-2 focus:ring-primary/50"
                      />
                      <Button
                        variant="secondary"
                        size="sm"
                        disabled={isLocked}
                        onClick={() => void handleSetManualLobby(match.id)}
                      >
                        Code speichern
                      </Button>
                    </div>
                  )}

                  {!isTerminal && (
                    <div className="mt-4 space-y-3">
                      <div className="space-y-2">
                        <label className="flex items-center gap-2 text-sm text-foreground">
                          <input
                            type="radio"
                            name={`group-winner-${match.id}`}
                            checked={selectedWinners[match.id] === match.team1_id}
                            onChange={() =>
                              setSelectedWinners((current) => ({ ...current, [match.id]: match.team1_id }))
                            }
                            className="accent-primary"
                          />
                          <span>{team1Name} gewonnen</span>
                        </label>
                        <label className="flex items-center gap-2 text-sm text-foreground">
                          <input
                            type="radio"
                            name={`group-winner-${match.id}`}
                            checked={selectedWinners[match.id] === match.team2_id}
                            onChange={() =>
                              setSelectedWinners((current) => ({ ...current, [match.id]: match.team2_id }))
                            }
                            className="accent-primary"
                          />
                          <span>{team2Name} gewonnen</span>
                        </label>
                      </div>

                      <Button
                        variant="primary"
                        size="sm"
                        disabled={!selectedWinners[match.id] || isLocked}
                        onClick={() => void handleSubmit(match.id)}
                      >
                        {isSubmitting ? 'Speichert...' : 'Ergebnis speichern'}
                      </Button>
                    </div>
                  )}

                  {message && (
                    <div
                      className={`mt-3 flex items-center gap-2 rounded-lg p-3 text-sm ${
                        message.kind === 'error'
                          ? 'border border-red-500/20 bg-red-500/10 text-red-400'
                          : 'border border-green-500/20 bg-green-500/10 text-green-400'
                      }`}
                    >
                      {message.kind === 'error' ? <AlertCircle size={16} /> : <CheckCircle2 size={16} />}
                      <span>{message.text}</span>
                    </div>
                  )}
                </div>
              )
            })}
          </div>
        </Card>
      ))}
    </div>
  )
}
