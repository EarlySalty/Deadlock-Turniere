import { useState } from 'react'
import Card from '@/components/ui/Card'
import Button from '@/components/ui/Button'
import {
  useVoiceMoveTeams,
  useVoiceMoveSammelpunkt,
  useVoiceMoveUser,
} from '@/hooks/useTournament'
import { Mic, MicOff, Users, AlertCircle, CheckCircle2 } from 'lucide-react'
import type { VoiceTarget } from '@/types/tournament'
import { summarizeVoiceMove, type VoiceMoveSummary } from './voiceMoveSummary'

const VOICE_TARGETS: { target: VoiceTarget; label: string }[] = [
  { target: 'sammelpunkt', label: 'Sammelpunkt' },
  { target: 'team1', label: 'Team 1 VC' },
  { target: 'team2', label: 'Team 2 VC' },
]

interface Props {
  tournamentId: number
  currentMatchId: number | null
}

function failureSummary(error: unknown): VoiceMoveSummary {
  return { kind: 'error', text: error instanceof Error ? error.message : 'Verschieben fehlgeschlagen' }
}

export default function VoiceChannelPanel({ tournamentId, currentMatchId }: Props) {
  const [manualDiscordId, setManualDiscordId] = useState('')
  const [manualTarget, setManualTarget] = useState<VoiceTarget>('sammelpunkt')
  const [status, setStatus] = useState<VoiceMoveSummary | null>(null)

  const moveTeams = useVoiceMoveTeams(tournamentId)
  const moveSammelpunkt = useVoiceMoveSammelpunkt(tournamentId)
  const moveUser = useVoiceMoveUser()

  const handleMoveTeams = async () => {
    if (!currentMatchId) return
    setStatus(null)
    try {
      const result = await moveTeams.mutateAsync(currentMatchId)
      setStatus(summarizeVoiceMove([result.team1, result.team2]))
    } catch (error) {
      setStatus(failureSummary(error))
    }
  }

  const handleMoveSammelpunkt = async () => {
    setStatus(null)
    try {
      setStatus(summarizeVoiceMove([await moveSammelpunkt.mutateAsync()]))
    } catch (error) {
      setStatus(failureSummary(error))
    }
  }

  const handleMoveUser = async () => {
    const discordId = manualDiscordId.trim()
    if (!discordId) return
    setStatus(null)
    try {
      const summary = summarizeVoiceMove([await moveUser.mutateAsync({ discordId, target: manualTarget })])
      setStatus(summary)
      if (summary.kind === 'success') setManualDiscordId('')
    } catch (error) {
      setStatus(failureSummary(error))
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
          {moveTeams.isPending ? 'Teams werden aufgeteilt...' : 'Runde starten: Teams aufteilen'}
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
              value={manualTarget}
              onChange={(e) => setManualTarget(e.target.value as VoiceTarget)}
              className="rounded-lg border border-border bg-background px-3 py-2 text-sm text-foreground focus:outline-none focus:ring-2 focus:ring-primary/50"
            >
              {VOICE_TARGETS.map((entry) => (
                <option key={entry.target} value={entry.target}>
                  {entry.label}
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
