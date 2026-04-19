import { useMemo, useState } from 'react'
import Button from '@/components/ui/Button'
import {
  useAssignMatchCaster,
  useAvailableCasters,
  useMatchCasters,
  useRemoveMatchCaster,
} from '@/hooks/useTournament'

interface CasterPanelProps {
  tournamentId: number
  matchId: number
}

export default function CasterPanel({ tournamentId, matchId }: CasterPanelProps) {
  const { data: availableCasters = [] } = useAvailableCasters()
  const { data: assignedCasters = [] } = useMatchCasters(tournamentId, matchId)
  const assignCaster = useAssignMatchCaster()
  const removeCaster = useRemoveMatchCaster()
  const [selectedCaster, setSelectedCaster] = useState('')

  const remainingCasters = useMemo(
    () => availableCasters.filter((caster) => !assignedCasters.some((assigned) => assigned.discord_id === caster.discord_id)),
    [availableCasters, assignedCasters],
  )

  const isBusy = assignCaster.isPending || removeCaster.isPending

  return (
    <div className="rounded-xl border border-border bg-background/50 p-4 space-y-3">
      <div>
        <h4 className="text-sm font-semibold text-foreground">Caster</h4>
        <p className="mt-1 text-xs text-muted">Zuweisen, entfernen und für den Match-Start bereithalten.</p>
      </div>

      <div className="flex flex-wrap gap-2">
        <select
          value={selectedCaster}
          onChange={(event) => setSelectedCaster(event.target.value)}
          className="min-w-[220px] rounded-lg border border-border bg-background px-3 py-2 text-sm text-foreground focus:outline-none focus:ring-2 focus:ring-primary/50"
        >
          <option value="">Caster wählen</option>
          {remainingCasters.map((caster) => (
            <option key={caster.discord_id} value={caster.discord_id}>
              {caster.display_name ?? caster.discord_id}
            </option>
          ))}
        </select>
        <Button
          variant="secondary"
          size="sm"
          disabled={isBusy || !selectedCaster}
          onClick={() => {
            assignCaster.mutate({ tournamentId, matchId, discordId: selectedCaster }, {
              onSuccess: () => setSelectedCaster(''),
            })
          }}
        >
          Zuweisen
        </Button>
      </div>

      {assignedCasters.length > 0 ? (
        <div className="flex flex-wrap gap-2">
          {assignedCasters.map((caster) => (
            <div
              key={caster.discord_id}
              className="flex items-center gap-2 rounded-full border border-border px-3 py-1 text-xs text-foreground"
            >
              <span>{caster.display_name ?? caster.discord_id}</span>
              <button
                type="button"
                className="text-muted hover:text-red-400"
                disabled={isBusy}
                onClick={() => removeCaster.mutate({ tournamentId, matchId, discordId: caster.discord_id })}
              >
                Entfernen
              </button>
            </div>
          ))}
        </div>
      ) : (
        <p className="text-xs text-muted">Noch keine Caster zugewiesen.</p>
      )}
    </div>
  )
}
