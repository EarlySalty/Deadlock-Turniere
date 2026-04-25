import { useMemo, useState } from 'react'
import Card from '@/components/ui/Card'
import Button from '@/components/ui/Button'
import {
  useAvailableCasters,
  useTournamentCasters,
  useAssignTournamentCaster,
  useRemoveTournamentCaster,
} from '@/hooks/useTournament'
import { Mic2, Plus, X, AlertCircle } from 'lucide-react'

interface TournamentCasterPanelProps {
  tournamentId: number
  readOnly?: boolean
}

export default function TournamentCasterPanel({
  tournamentId,
  readOnly = false,
}: TournamentCasterPanelProps) {
  const { data: availableCasters = [], isLoading: loadingPool, error: poolError } =
    useAvailableCasters()
  const { data: assignedCasters = [], isLoading: loadingAssigned } =
    useTournamentCasters(tournamentId)
  const assignCaster = useAssignTournamentCaster()
  const removeCaster = useRemoveTournamentCaster()
  const [selected, setSelected] = useState('')

  const remainingCasters = useMemo(
    () =>
      availableCasters.filter(
        (caster) => !assignedCasters.some((a) => a.discord_id === caster.discord_id),
      ),
    [availableCasters, assignedCasters],
  )

  const isBusy = assignCaster.isPending || removeCaster.isPending

  return (
    <Card className="space-y-4 p-5">
      <div className="flex items-center gap-2">
        <Mic2 size={16} className="text-primary" />
        <h3 className="font-semibold text-foreground">Caster</h3>
      </div>
      <p className="text-xs text-muted">
        Caster werden auf Turnier-Ebene gepflegt. Wer hier eingetragen ist, wird automatisch
        in jeden Match-Channel eingeladen. Pool kommt aus der Discord-Caster-Rolle.
      </p>

      {poolError && (
        <div className="flex items-center gap-2 rounded-lg border border-red-500/20 bg-red-500/10 p-3 text-sm text-red-400">
          <AlertCircle size={16} />
          <span>Caster-Pool konnte nicht geladen werden — Discord-Bot offline?</span>
        </div>
      )}

      {!readOnly && (
        <div className="flex flex-wrap items-end gap-2">
          <div className="flex flex-col gap-1">
            <label className="text-xs text-muted" htmlFor={`caster-select-${tournamentId}`}>
              Caster aus Discord-Rolle wählen
            </label>
            <select
              id={`caster-select-${tournamentId}`}
              value={selected}
              onChange={(event) => setSelected(event.target.value)}
              className="min-w-[260px] rounded-lg border border-border bg-background px-3 py-2 text-sm text-foreground focus:outline-none focus:ring-2 focus:ring-primary/50"
              disabled={loadingPool || remainingCasters.length === 0}
            >
              <option value="">
                {loadingPool
                  ? 'Lade Caster...'
                  : remainingCasters.length === 0
                    ? 'Alle verfügbaren Caster zugewiesen'
                    : 'Caster wählen'}
              </option>
              {remainingCasters.map((caster) => (
                <option key={caster.discord_id} value={caster.discord_id}>
                  {caster.display_name ?? caster.discord_id}
                </option>
              ))}
            </select>
          </div>
          <Button
            variant="primary"
            size="sm"
            disabled={isBusy || !selected}
            onClick={() => {
              assignCaster.mutate(
                { tournamentId, discordId: selected },
                { onSuccess: () => setSelected('') },
              )
            }}
          >
            <Plus size={14} />
            Zuweisen
          </Button>
        </div>
      )}

      <div className="space-y-2">
        <p className="text-xs font-medium uppercase tracking-wider text-muted">
          Aktiv für dieses Turnier ({assignedCasters.length})
        </p>
        {loadingAssigned ? (
          <p className="text-xs text-muted">Lade...</p>
        ) : assignedCasters.length === 0 ? (
          <p className="text-xs text-muted">
            Noch keine Caster eingetragen — die Matches werden ohne Caster-Einladung
            erstellt.
          </p>
        ) : (
          <div className="flex flex-wrap gap-2">
            {assignedCasters.map((caster) => (
              <div
                key={caster.discord_id}
                className="flex items-center gap-2 rounded-full border border-border bg-background/40 px-3 py-1.5 text-xs text-foreground"
              >
                <Mic2 size={12} className="text-primary" />
                <span>{caster.display_name ?? caster.discord_id}</span>
                {!readOnly && (
                  <button
                    type="button"
                    className="ml-1 text-muted hover:text-red-400"
                    disabled={isBusy}
                    onClick={() =>
                      removeCaster.mutate({ tournamentId, discordId: caster.discord_id })
                    }
                    aria-label={`Caster ${caster.display_name ?? caster.discord_id} entfernen`}
                  >
                    <X size={12} />
                  </button>
                )}
              </div>
            ))}
          </div>
        )}
      </div>
    </Card>
  )
}
