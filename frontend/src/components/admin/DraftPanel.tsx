import { useState } from 'react'
import Card from '@/components/ui/Card'
import Button from '@/components/ui/Button'
import {
  useDraftHeroes,
  useDraftSession,
  useStartDraft,
  useSubmitDraftAction,
} from '@/hooks/useTournament'
import type { DraftState } from '@/types/tournament'
import { Shield, Swords, AlertCircle } from 'lucide-react'

interface Props {
  matchId: number
  team1Name: string
  team2Name: string
}

export default function DraftPanel({ matchId, team1Name, team2Name }: Props) {
  const [sessionId, setSessionId] = useState<number | null>(null)
  const heroesQuery = useDraftHeroes()
  const sessionQuery = useDraftSession(sessionId)
  const startDraftMutation = useStartDraft()
  const submitAction = useSubmitDraftAction(sessionId ?? 0)

  const draft: DraftState | undefined = sessionQuery.data
  // Die Heldenliste kommt seit der Live-Quelle als Objekte (id/name/image_url).
  // Dieses Panel arbeitet mit Namen; die Bilder nutzt das oeffentliche Board.
  const heroes: string[] = (heroesQuery.data?.heroes ?? []).map((h) => h.name)
  const bannedSet = new Set(draft?.bans ?? [])
  const pickedSet = new Set([...(draft?.picks_team1 ?? []), ...(draft?.picks_team2 ?? [])])

  const handleStart = async () => {
    const result = await startDraftMutation.mutateAsync(matchId)
    setSessionId(result.id)
  }

  const handlePick = async (hero: string) => {
    if (!sessionId) return
    await submitAction.mutateAsync({ heroName: hero, takenBy: 'admin' })
  }

  if (!sessionId) {
    return (
      <Card className="p-5">
        <div className="flex items-center justify-between gap-3">
          <div className="flex items-center gap-2">
            <Swords size={16} className="text-primary" />
            <span className="font-medium text-foreground">Hero-Draft</span>
          </div>
          <Button
            variant="secondary"
            size="sm"
            disabled={startDraftMutation.isPending}
            onClick={() => void handleStart()}
          >
            Draft starten
          </Button>
        </div>
      </Card>
    )
  }

  const currentTeamName = draft?.current_team_slot === 1 ? team1Name : team2Name
  const currentAction = draft?.current_action_type
  const statusLabel = currentAction
    ? `${currentTeamName} ${currentAction === 'ban' ? 'bannt' : 'pickt'}`
    : 'Draft abgeschlossen'

  return (
    <Card className="space-y-4 p-5">
      <div className="flex items-center justify-between gap-3">
        <div className="flex items-center gap-2">
          <Swords size={16} className="text-primary" />
          <span className="font-semibold text-foreground">Hero-Draft</span>
        </div>
        <span
          className={`text-sm font-medium ${
            currentAction === 'ban'
              ? 'text-red-400'
              : currentAction === 'pick'
                ? 'text-green-400'
                : 'text-muted'
          }`}
        >
          {statusLabel}
        </span>
      </div>

      <div className="grid grid-cols-2 gap-4 text-sm">
        <div>
          <p className="mb-1 text-xs font-medium uppercase tracking-wider text-muted">
            Bans ({draft?.bans.length ?? 0}/6)
          </p>
          <p className="text-red-400">{draft?.bans.join(', ') || '—'}</p>
        </div>
        <div className="col-span-2 grid grid-cols-2 gap-2">
          <div>
            <p className="mb-1 text-xs font-medium uppercase tracking-wider text-muted">
              {team1Name} ({draft?.picks_team1.length ?? 0}/6)
            </p>
            <p className="text-foreground">{draft?.picks_team1.join(', ') || '—'}</p>
          </div>
          <div>
            <p className="mb-1 text-xs font-medium uppercase tracking-wider text-muted">
              {team2Name} ({draft?.picks_team2.length ?? 0}/6)
            </p>
            <p className="text-foreground">{draft?.picks_team2.join(', ') || '—'}</p>
          </div>
        </div>
      </div>

      {draft?.status === 'in_progress' && (
        <div>
          <p className="mb-2 text-xs font-medium uppercase tracking-wider text-muted">
            Helden wählen
          </p>
          <div className="flex flex-wrap gap-1.5">
            {heroes.map((hero) => {
              const isBanned = bannedSet.has(hero)
              const isPicked = pickedSet.has(hero)
              const disabled = isBanned || isPicked || submitAction.isPending

              return (
                <Button
                  key={hero}
                  variant="secondary"
                  size="sm"
                  disabled={disabled}
                  onClick={() => void handlePick(hero)}
                  className={`px-2.5 py-1 text-xs ${
                    isBanned
                      ? 'border-red-500/30 bg-red-500/10 text-red-400 line-through hover:bg-red-500/10'
                      : isPicked
                        ? 'border-border bg-muted/20 text-muted line-through hover:bg-muted/20'
                        : 'hover:border-primary/50 hover:bg-primary/10 hover:text-primary'
                  }`}
                >
                  {isBanned && <Shield size={10} />}
                  {hero}
                </Button>
              )
            })}
          </div>
        </div>
      )}

      {draft?.status === 'completed' && (
        <div className="flex items-center gap-2 rounded-lg border border-green-500/20 bg-green-500/10 p-3 text-sm text-green-400">
          <AlertCircle size={16} />
          <span>Draft abgeschlossen</span>
        </div>
      )}
    </Card>
  )
}
