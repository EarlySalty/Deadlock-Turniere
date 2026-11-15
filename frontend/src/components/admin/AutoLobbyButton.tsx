import { useState } from 'react'
import { Sparkles, AlertCircle, CheckCircle2 } from 'lucide-react'
import Button from '@/components/ui/Button'
import { useTriggerAutoLobby } from '@/hooks/useTournament'
import { pluralize } from '@/lib/countLabel'
import type { AutoLobbyRun } from '@/types/tournament'

interface AutoLobbyButtonProps {
  tournamentId: number
  enabled: boolean
}

type Feedback = { kind: 'success' | 'error'; text: string }

function runFeedback(run: AutoLobbyRun): Feedback {
  if (!run.active) {
    return { kind: 'error', text: 'Keine Lobby erstellt: Auto-Lobby ist für dieses Turnier aus oder es ist ein Testturnier.' }
  }
  if (run.created === 0 && run.failed === 0) {
    return { kind: 'success', text: 'Keine spielbereiten Matches ohne Lobby gefunden.' }
  }
  const created = `${pluralize(run.created, 'Lobby', 'Lobbys')} erstellt`
  return run.failed > 0
    ? { kind: 'error', text: `${created}, ${run.failed} fehlgeschlagen` }
    : { kind: 'success', text: created }
}

export default function AutoLobbyButton({ tournamentId, enabled }: AutoLobbyButtonProps) {
  const triggerMutation = useTriggerAutoLobby()
  const [feedback, setFeedback] = useState<Feedback | null>(null)

  const handleClick = () => {
    setFeedback(null)
    triggerMutation.mutate(tournamentId, { onSuccess: (run) => setFeedback(runFeedback(run)) })
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
          Auto-Lobby ist deaktiviert. Der Button erstellt dann keine Lobbys.
        </span>
      )}

      {feedback && !triggerMutation.isError && (
        <span
          role="status"
          className={`flex items-center gap-1 text-xs ${feedback.kind === 'error' ? 'text-red-400' : 'text-success'}`}
        >
          {feedback.kind === 'error' ? <AlertCircle size={12} /> : <CheckCircle2 size={12} />}
          {feedback.text}
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
