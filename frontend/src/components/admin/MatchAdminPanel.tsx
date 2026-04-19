import { useState } from 'react'
import Card from '@/components/ui/Card'
import Button from '@/components/ui/Button'
import ManualResultForm from '@/components/admin/ManualResultForm'
import DraftPanel from '@/components/admin/DraftPanel'
import CasterPanel from '@/components/admin/CasterPanel'
import {
  useCreateLobby,
  useFetchMatchResult,
  useLeaveLobby,
  useResetMatch,
  useSetManualMatchLobby,
  useStartMatch,
} from '@/hooks/useTournament'
import type { BracketMatch, MatchGame, Team } from '@/types/tournament'
import {
  AlertCircle,
  Copy,
  DoorOpen,
  KeyRound,
  CheckCircle2,
  Play,
  Radio,
  RefreshCcw,
  Swords,
} from 'lucide-react'

interface MatchAdminPanelProps {
  tournamentId: number
  matches: BracketMatch[]
  teams: Team[]
  onRefresh?: () => void
  allowManualOverride?: boolean
}

type ActionKey = 'create' | 'start' | 'result' | 'leave'
type MessageKind = 'success' | 'error'

interface ActiveAction {
  matchId: number
  action: ActionKey
}

interface MatchMessage {
  kind: MessageKind
  text: string
}

function formatDuration(seconds: number): string {
  const mins = Math.floor(seconds / 60)
  const secs = seconds % 60
  return `${mins}:${String(secs).padStart(2, '0')} min`
}

function teamName(teamId: number | null, teams: Team[]): string {
  if (teamId === null) return 'TBD'
  return teams.find((team) => team.id === teamId)?.name ?? `Team #${teamId}`
}

function statusLabel(match: BracketMatch): string {
  switch (match.status) {
    case 'checkin':
      return 'Check-in'
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

function isTerminalMatch(match: BracketMatch): boolean {
  return ['completed', 'forfeit', 'cancelled'].includes(match.status)
}

function matchHeading(match: BracketMatch): string {
  return `Runde ${match.round} / Spiel ${match.position + 1}`
}

export default function MatchAdminPanel({
  tournamentId,
  matches,
  teams,
  onRefresh,
  allowManualOverride = false,
}: MatchAdminPanelProps) {
  const createLobbyMutation = useCreateLobby()
  const startMatchMutation = useStartMatch()
  const fetchResultMutation = useFetchMatchResult()
  const leaveLobbyMutation = useLeaveLobby()
  const resetMatchMutation = useResetMatch()
  const setManualLobbyMutation = useSetManualMatchLobby()

  const [activeAction, setActiveAction] = useState<ActiveAction | null>(null)
  const [messages, setMessages] = useState<Record<number, MatchMessage>>({})
  const [copyFeedback, setCopyFeedback] = useState<Record<number, boolean>>({})
  const [manualCodes, setManualCodes] = useState<Record<number, string>>({})

  const actionableMatches = matches.filter((match) => match.team1_id !== null && match.team2_id !== null)

  if (actionableMatches.length === 0) {
    return (
      <Card className="p-5">
        <div className="flex items-center gap-2 text-muted">
          <Swords size={18} />
          <span>Keine Matches bereit — warte auf Gegner-Zuweisung.</span>
        </div>
      </Card>
    )
  }

  const isLocked = Boolean(activeAction)

  const getActiveActionType = (matchId: number): ActionKey | null => {
    if (!activeAction) return null
    return activeAction.matchId === matchId ? activeAction.action : null
  }

  const setMessage = (matchId: number, kind: MessageKind, text: string) => {
    setMessages((current) => ({ ...current, [matchId]: { kind, text } }))
  }

  const clearMessage = (matchId: number) => {
    setMessages((current) => {
      const next = { ...current }
      delete next[matchId]
      return next
    })
  }

  const handleRefresh = () => {
    onRefresh?.()
  }

  const handleCreateLobby = async (matchId: number) => {
    if (isLocked) return
    setActiveAction({ matchId, action: 'create' })
    clearMessage(matchId)
    try {
      await createLobbyMutation.mutateAsync({ tournamentId, matchId })
      setMessage(matchId, 'success', 'Lobby erstellt und Party-Code gespeichert.')
      handleRefresh()
    } catch (err) {
      setMessage(
        matchId,
        'error',
        err instanceof Error ? err.message : 'Lobby konnte nicht erstellt werden'
      )
    } finally {
      setActiveAction(null)
    }
  }

  const handleStartMatch = async (matchId: number) => {
    if (isLocked) return
    setActiveAction({ matchId, action: 'start' })
    clearMessage(matchId)
    try {
      await startMatchMutation.mutateAsync({ tournamentId, matchId })
      setMessage(matchId, 'success', 'Match erfolgreich gestartet.')
      handleRefresh()
    } catch (err) {
      setMessage(
        matchId,
        'error',
        err instanceof Error ? err.message : 'Match konnte nicht gestartet werden'
      )
    } finally {
      setActiveAction(null)
    }
  }

  const handleFetchResult = async (matchId: number) => {
    if (isLocked) return
    setActiveAction({ matchId, action: 'result' })
    clearMessage(matchId)
    try {
      await fetchResultMutation.mutateAsync({ tournamentId, matchId })
      setMessage(matchId, 'success', 'Match-Ergebnis übernommen.')
      handleRefresh()
    } catch (err) {
      setMessage(
        matchId,
        'error',
        err instanceof Error ? err.message : 'Match-Ergebnis konnte nicht geladen werden'
      )
    } finally {
      setActiveAction(null)
    }
  }

  const handleLeaveLobby = async (matchId: number) => {
    if (isLocked) return
    setActiveAction({ matchId, action: 'leave' })
    clearMessage(matchId)
    try {
      await leaveLobbyMutation.mutateAsync({ tournamentId, matchId })
      setMessage(matchId, 'success', 'Bot hat die Lobby verlassen.')
      handleRefresh()
    } catch (err) {
      setMessage(
        matchId,
        'error',
        err instanceof Error ? err.message : 'Lobby konnte nicht verlassen werden'
      )
    } finally {
      setActiveAction(null)
    }
  }

  const handleResetMatch = async (matchId: number) => {
    if (isLocked || !window.confirm('Match wirklich zurücksetzen? Lobby-Daten und Match-Channel werden gelöscht.')) return
    setActiveAction({ matchId, action: 'leave' })
    clearMessage(matchId)
    try {
      await resetMatchMutation.mutateAsync({ tournamentId, matchId })
      setMessage(matchId, 'success', 'Match wurde auf pending zurückgesetzt.')
      handleRefresh()
    } catch (err) {
      setMessage(matchId, 'error', err instanceof Error ? err.message : 'Match konnte nicht zurückgesetzt werden')
    } finally {
      setActiveAction(null)
    }
  }

  const handleSetManualLobby = async (matchId: number) => {
    if (isLocked) return
    const partyCode = manualCodes[matchId]?.trim()
    if (!partyCode) {
      setMessage(matchId, 'error', 'Bitte zuerst einen Party-Code eingeben.')
      return
    }
    setActiveAction({ matchId, action: 'create' })
    clearMessage(matchId)
    try {
      await setManualLobbyMutation.mutateAsync({ tournamentId, matchId, partyCode })
      setMessage(matchId, 'success', 'Party-Code wurde manuell gespeichert.')
      handleRefresh()
    } catch (err) {
      setMessage(matchId, 'error', err instanceof Error ? err.message : 'Party-Code konnte nicht gespeichert werden')
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
      setMessage(matchId, 'error', 'Party-Code konnte nicht kopiert werden.')
    }
  }

  return (
    <div className="space-y-4">
      {actionableMatches.map((match) => {
        const activeActionType = getActiveActionType(match.id)
        const isBusy = isLocked
        const canCreateLobby = !match.steam_party_id && ['pending', 'checkin'].includes(match.status)
        const canStartMatch = match.status === 'lobby_created' && Boolean(match.steam_party_id)
        const canFetchResult = match.status === 'in_progress' && (Boolean(match.deadlock_match_id) || Boolean(match.steam_party_id))
        const canLeaveLobby =
          ['lobby_created', 'in_progress'].includes(match.status) && Boolean(match.steam_party_id)
        const canReset = !['completed', 'forfeit', 'cancelled'].includes(match.status)
        const message = messages[match.id]

        return (
          <Card key={match.id} className="p-5 space-y-4">
            <div className="flex flex-col gap-3 lg:flex-row lg:items-start lg:justify-between">
              <div className="space-y-2">
                <div className="flex items-center gap-2 text-sm text-muted">
                  <Swords size={16} className="text-primary" />
                  <span>{matchHeading(match)}</span>
                </div>
                <h3 className="text-lg font-semibold text-foreground">
                  {teamName(match.team1_id, teams)} vs {teamName(match.team2_id, teams)}
                </h3>
                {(match.series_wins_team1 + match.series_wins_team2) > 0 && (
                  <div className="text-sm font-medium text-foreground">
                    Serie: {teamName(match.team1_id, teams)} {match.series_wins_team1} : {match.series_wins_team2} {teamName(match.team2_id, teams)}
                  </div>
                )}
                <div className="flex flex-wrap gap-3 text-xs text-muted">
                  <span>Status: {statusLabel(match)}</span>
                  {match.deadlock_match_id && <span>Deadlock Match-ID: {match.deadlock_match_id}</span>}
                  {match.match_duration_s !== null && <span>Dauer: {formatDuration(match.match_duration_s)}</span>}
                </div>
              </div>

              {match.party_code && (
                <div className="min-w-[220px] rounded-xl border border-primary/20 bg-primary/10 px-4 py-3">
                  <div className="flex items-center gap-2 text-xs uppercase tracking-wider text-primary/80">
                    <KeyRound size={14} />
                    Party-Code
                  </div>
                  <div className="mt-2 text-2xl font-bold tracking-[0.2em] text-foreground">
                    {match.party_code}
                  </div>
                  <Button
                    variant="secondary"
                    size="sm"
                    className="mt-3 w-full"
                    onClick={() => void handleCopyCode(match.id, match.party_code!)}
                  >
                    <Copy size={14} />
                    {copyFeedback[match.id] ? 'Kopiert' : 'Code kopieren'}
                  </Button>
                </div>
              )}
            </div>

            <div className="flex flex-wrap gap-2">
              {canCreateLobby && (
                <Button
                  variant="primary"
                  size="sm"
                  disabled={isBusy}
                  onClick={() => void handleCreateLobby(match.id)}
                >
                  <Radio size={14} />
                  {activeActionType === 'create' ? 'Lobby wird erstellt...' : 'Lobby erstellen'}
                </Button>
              )}

              {canStartMatch && (
                <Button
                  variant="primary"
                  size="sm"
                  disabled={isBusy}
                  onClick={() => void handleStartMatch(match.id)}
                >
                  <Play size={14} />
                  {activeActionType === 'start' ? "Los geht's..." : "Los geht's"}
                </Button>
              )}

              {canFetchResult && (
                <Button
                  variant="secondary"
                  size="sm"
                  disabled={isBusy}
                  onClick={() => void handleFetchResult(match.id)}
                >
                  <RefreshCcw size={14} />
                  {activeActionType === 'result' ? 'Ergebnis wird geladen...' : 'Ergebnis abrufen'}
                </Button>
              )}

              {canLeaveLobby && (
                <Button
                  variant="ghost"
                  size="sm"
                  disabled={isBusy}
                  onClick={() => void handleLeaveLobby(match.id)}
                >
                  <DoorOpen size={14} />
                  {activeActionType === 'leave' ? 'Lobby wird verlassen...' : 'Lobby verlassen'}
                </Button>
              )}

              {canReset && (
                <Button
                  variant="ghost"
                  size="sm"
                  disabled={isBusy}
                  onClick={() => void handleResetMatch(match.id)}
                >
                  <RefreshCcw size={14} />
                  Match zurücksetzen
                </Button>
              )}
            </div>

            {!match.steam_party_id && !isTerminalMatch(match) && (
              <div className="grid gap-2 sm:grid-cols-[1fr_auto]">
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
                  disabled={isBusy}
                  onClick={() => void handleSetManualLobby(match.id)}
                >
                  Code speichern
                </Button>
              </div>
            )}

            <CasterPanel tournamentId={tournamentId} matchId={match.id} />

            {!match.steam_party_id && !isTerminalMatch(match) && (
              <div className="rounded-lg border border-amber-500/20 bg-amber-500/10 p-3 text-xs text-amber-300">
                Dieses Bracket-Match kann auch ohne Bot-Lobby gespielt und unten per manuellem Ergebnis abgeschlossen werden.
              </div>
            )}

            {match.games && match.games.length > 0 && (
              <div className="border-t border-border pt-3">
                <p className="mb-2 text-xs font-medium uppercase tracking-wider text-muted">Spiele</p>
                <div className="flex flex-wrap gap-2">
                  {match.games.map((game: MatchGame) => (
                    <div
                      key={game.id}
                      className={`rounded-lg border px-3 py-2 text-xs ${
                        game.status === 'completed'
                          ? 'border-green-500/20 bg-green-500/10 text-green-400'
                          : 'border-border bg-background text-muted'
                      }`}
                    >
                      Spiel {game.game_number}
                      {game.winner_team &&
                        ` — ${
                          game.winner_team === 1
                            ? teamName(match.team1_id, teams)
                            : teamName(match.team2_id, teams)
                        } gewinnt`}
                      {game.duration_s && ` (${Math.floor(game.duration_s / 60)}m)`}
                    </div>
                  ))}
                </div>
              </div>
            )}

            {match.status === 'completed' && match.match_stats && (() => {
              try {
                const stats = JSON.parse(match.match_stats) as {
                  players?: Array<{
                    hero_id?: number
                    kills?: number
                    deaths?: number
                    assists?: number
                  }>
                }
                if (!stats.players?.length) return null
                return (
                  <details className="border-t border-border pt-3">
                    <summary className="cursor-pointer text-xs font-medium uppercase tracking-wider text-muted hover:text-foreground">
                      Match-Stats anzeigen
                    </summary>
                    <div className="mt-2 overflow-x-auto">
                      <table className="w-full text-xs">
                        <thead>
                          <tr className="text-left text-muted">
                            <th className="pb-1 pr-3">Hero ID</th>
                            <th className="pb-1 pr-3">K</th>
                            <th className="pb-1 pr-3">D</th>
                            <th className="pb-1">A</th>
                          </tr>
                        </thead>
                        <tbody>
                          {stats.players.map((player, index) => (
                            <tr key={index} className="border-t border-border/50 text-foreground">
                              <td className="py-1 pr-3">{player.hero_id ?? '—'}</td>
                              <td className="py-1 pr-3">{player.kills ?? '—'}</td>
                              <td className="py-1 pr-3">{player.deaths ?? '—'}</td>
                              <td className="py-1">{player.assists ?? '—'}</td>
                            </tr>
                          ))}
                        </tbody>
                      </table>
                    </div>
                  </details>
                )
              } catch {
                return null
              }
            })()}

            {message && (
              <div
                className={`flex items-center gap-2 rounded-lg p-3 text-sm ${
                  message.kind === 'error'
                    ? 'border border-red-500/20 bg-red-500/10 text-red-400'
                    : 'border border-green-500/20 bg-green-500/10 text-green-400'
                }`}
              >
                {message.kind === 'error' ? <AlertCircle size={16} /> : <CheckCircle2 size={16} />}
                <span>{message.text}</span>
              </div>
            )}

            {!isTerminalMatch(match) && (
              <ManualResultForm
                tournamentId={tournamentId}
                match={match}
                teams={teams}
                onSuccess={handleRefresh}
              />
            )}

            {isTerminalMatch(match) && allowManualOverride && (
              <ManualResultForm
                tournamentId={tournamentId}
                match={match}
                teams={teams}
                onSuccess={handleRefresh}
                allowOverride
              />
            )}

            {!isTerminalMatch(match) && (
              <DraftPanel
                matchId={match.id}
                team1Name={teamName(match.team1_id, teams)}
                team2Name={teamName(match.team2_id, teams)}
              />
            )}
          </Card>
        )
      })}
    </div>
  )
}
