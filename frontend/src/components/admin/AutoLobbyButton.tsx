import { useState } from 'react'
import { Sparkles, AlertCircle, CheckCircle2 } from 'lucide-react'
import Button from '@/components/ui/Button'
import { useTriggerAutoLobby } from '@/hooks/useTournament'

interface AutoLobbyButtonProps {
  tournamentId: number
  /** Wenn false, ist Auto-Lobby für das Turnier deaktiviert — Button greift trotzdem als manueller Trigger. */
  enabled: boolean
}

export default function AutoLobbyButton({ tournamentId, enabled }: AutoLobbyButtonProps) {
  const triggerMutation = useTriggerAutoLobby()
  const [feedback, setFeedback] = useState<string | null>(null)

  const handleClick = () => {
    setFeedback(null)
    triggerMutation.mutate(tournamentId, {
      onSuccess: (data) => {
        setFeedback(
          `${data.created} Lobby${data.created === 1 ? '' : 's'} erstellt` +
            (data.failed > 0 ? ` · ${data.failed} fehlgeschlagen` : '') +
            (data.skipped > 0 ? ` · ${data.skipped} übersprungen` : ''),
        )
      },
    })
  }

  return (
    <div className="flex flex-wrap items-center gap-3">
      <Button
        variant="secondary"
        size="sm"
        disabled={triggerMutation.isPending}
        onClick={handleClick}
      >
        <Sparkles size={14} />
        {triggerMutation.isPending ? 'Lobbys werden erstellt…' : 'Auto-Lobby jetzt ausführen'}
      </Button>

      {!enabled && (
        <span className="text-xs text-amber-300">
          Auto-Lobby ist deaktiviert — Button startet nur manuell.
        </span>
      )}

      {feedback && !triggerMutation.isError && (
        <span className="flex items-center gap-1 text-xs text-success">
          <CheckCircle2 size={12} />
          {feedback}
        </span>
      )}

      {triggerMutation.isError && (
        <span className="flex items-center gap-1 text-xs text-red-400">
          <AlertCircle size={12} />
          {triggerMutation.error instanceof Error
            ? triggerMutation.error.message
            : 'Auto-Lobby fehlgeschlagen'}
        </span>
      )}
    </div>
  )
}
