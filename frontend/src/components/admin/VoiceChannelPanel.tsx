import { useState } from 'react'
import Card from '@/components/ui/Card'
import Button from '@/components/ui/Button'
import {
  useVoiceMoveTeams,
  useVoiceMoveSammelpunkt,
  useVoiceMoveUser,
} from '@/hooks/useTournament'
import { Mic, MicOff, Users, AlertCircle, CheckCircle2 } from 'lucide-react'

const VOICE_CHANNELS: { id: string; label: string }[] = [
  { id: '1426160735469174875', label: 'Sammelpunkt' },
  { id: '1462434609563173019', label: 'Team 1 VC' },
  { id: '1462434639858897017', label: 'Team 2 VC' },
]

interface Props {
  tournamentId: number
  currentMatchId: number | null
}

type StatusMsg = { kind: 'success' | 'error'; text: string }

export default function VoiceChannelPanel({ tournamentId, currentMatchId }: Props) {
  const [manualDiscordId, setManualDiscordId] = useState('')
  const [manualChannelId, setManualChannelId] = useState<string>(VOICE_CHANNELS[0].id)
  const [status, setStatus] = useState<StatusMsg | null>(null)

  const moveTeams = useVoiceMoveTeams(tournamentId)
  const moveSammelpunkt = useVoiceMoveSammelpunkt(tournamentId)
  const moveUser = useVoiceMoveUser()

  const handleMoveTeams = async () => {
    if (!currentMatchId) return
    setStatus(null)
    try {
      const result = await moveTeams.mutateAsync(currentMatchId)
      const total = result.team1.moved.length + result.team2.moved.length
      const failed = result.team1.failed.length + result.team2.failed.length
      setStatus({
        kind: failed > 0 ? 'error' : 'success',
        text:
          failed > 0
            ? `${total} verschoben, ${failed} fehlgeschlagen (User evtl. nicht im Voice)`
            : `Teams aufgeteilt — ${total} Spieler verschoben`,
      })
    } catch {
      setStatus({ kind: 'error', text: 'Verschieben fehlgeschlagen' })
    }
  }

  const handleMoveSammelpunkt = async () => {
    setStatus(null)
    try {
      const result = await moveSammelpunkt.mutateAsync()
      const failed = result.failed.length
      setStatus({
        kind: failed > 0 ? 'error' : 'success',
        text: failed > 0 ? `Teilweise fehlgeschlagen (${failed} User)` : 'Alle in Sammelpunkt verschoben',
      })
    } catch {
      setStatus({ kind: 'error', text: 'Verschieben fehlgeschlagen' })
    }
  }

  const handleMoveUser = async () => {
    if (!manualDiscordId.trim()) return
    setStatus(null)
    try {
      await moveUser.mutateAsync({ discordId: manualDiscordId.trim(), channelId: manualChannelId })
      setStatus({ kind: 'success', text: `User ${manualDiscordId} verschoben` })
      setManualDiscordId('')
    } catch {
      setStatus({ kind: 'error', text: 'User konnte nicht verschoben werden (im Voice?)' })
    }
  }

  const isBusy = moveTeams.isPending || moveSammelpunkt.isPending || moveUser.isPending

  return (
    <Card className="space-y-4 p-5">
      <div className="flex items-center gap-2">
        <Mic size={16} className="text-primary" />
        <h3 className="font-semibold text-foreground">Voice-Kanal-Steuerung</h3>
      </div>

      <div className="flex flex-wrap gap-2">
        <Button
          variant="primary"
          size="sm"
          disabled={isBusy || !currentMatchId}
          onClick={() => void handleMoveTeams()}
        >
          <Users size={14} />
          {moveTeams.isPending ? 'Teams werden aufgeteilt...' : 'Runde starten — Teams aufteilen'}
        </Button>
        <Button
          variant="secondary"
          size="sm"
          disabled={isBusy}
          onClick={() => void handleMoveSammelpunkt()}
        >
          <MicOff size={14} />
          {moveSammelpunkt.isPending ? 'Wird verschoben...' : 'Alle in Sammelpunkt'}
        </Button>
      </div>

      <div className="border-t border-border pt-4">
        <p className="mb-2 text-xs font-medium uppercase tracking-wider text-muted">
          Manuell verschieben
        </p>
        <div className="flex flex-wrap items-end gap-2">
          <div className="flex flex-col gap-1">
            <label className="text-xs text-muted" htmlFor="manual-discord-id">
              Discord ID
            </label>
            <input
              id="manual-discord-id"
              type="text"
              placeholder="z.B. 123456789012345678"
              value={manualDiscordId}
              onChange={(e) => setManualDiscordId(e.target.value)}
              className="rounded-lg border border-border bg-background px-3 py-2 text-sm text-foreground placeholder:text-muted focus:outline-none focus:ring-2 focus:ring-primary/50"
            />
          </div>
          <div className="flex flex-col gap-1">
            <label className="text-xs text-muted" htmlFor="manual-channel-id">
              Ziel-Kanal
            </label>
            <select
              id="manual-channel-id"
              value={manualChannelId}
              onChange={(e) => setManualChannelId(e.target.value)}
              className="rounded-lg border border-border bg-background px-3 py-2 text-sm text-foreground focus:outline-none focus:ring-2 focus:ring-primary/50"
            >
              {VOICE_CHANNELS.map((channel) => (
                <option key={channel.id} value={channel.id}>
                  {channel.label}
                </option>
              ))}
            </select>
          </div>
          <Button
            variant="secondary"
            size="sm"
            disabled={isBusy || !manualDiscordId.trim()}
            onClick={() => void handleMoveUser()}
          >
            Verschieben
          </Button>
        </div>
      </div>

      {status && (
        <div
          className={`flex items-center gap-2 rounded-lg p-3 text-sm ${
            status.kind === 'error'
              ? 'border border-red-500/20 bg-red-500/10 text-red-400'
              : 'border border-green-500/20 bg-green-500/10 text-green-400'
          }`}
        >
          {status.kind === 'error' ? <AlertCircle size={16} /> : <CheckCircle2 size={16} />}
          <span>{status.text}</span>
        </div>
      )}
    </Card>
  )
}
