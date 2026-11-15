import { AlertTriangle, Check, CheckCircle2, Radio, X } from 'lucide-react'
import Card from '@/components/ui/Card'
import Button from '@/components/ui/Button'
import {
  useActionItems,
  useConfirmResultReport,
  useRejectResultReport,
} from '@/hooks/useTournament'
import { latestActionError } from '@/lib/latestError'
import type { ActionItem } from '@/types/tournament'
import { teamNameOr } from '@/lib/teamName'

interface LeitstandProps {
  tournamentId: number
}

function ActionRow({
  item,
  tournamentId,
}: {
  item: ActionItem
  tournamentId: number
}) {
  const confirm = useConfirmResultReport(tournamentId)
  const reject = useRejectResultReport(tournamentId)
  const busy = confirm.isPending || reject.isPending
  const actionError = latestActionError([confirm, reject])

  const matchup = `${teamNameOr(item.team1_name, 'Team 1')} vs ${teamNameOr(item.team2_name, 'Team 2')}`

  return (
    <div className="rounded-lg border border-border bg-card-hover/40 p-3">
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0">
          <div className="flex items-center gap-2 text-sm font-medium text-foreground">
            <span>Match {item.match_id}</span>
            <span className="text-[10px] uppercase tracking-wider text-muted">
              Runde {item.match_round}
            </span>
            <span
              className={`flex items-center gap-1 rounded-full px-1.5 py-0.5 text-[9px] ${
                item.on_stream
                  ? 'bg-primary/15 text-primary'
                  : 'bg-muted/15 text-muted'
              }`}
            >
              <Radio size={9} />
              {item.on_stream ? 'Stream' : 'Parallel'}
            </span>
          </div>
          <div className="mt-0.5 truncate text-xs text-muted">{matchup}</div>

          {item.is_no_show ? (
            <div className="mt-1.5 flex items-center gap-1.5 text-xs">
              <AlertTriangle
                size={13}
                className={item.grace_expired ? 'text-red-400' : 'text-warning'}
              />
              <span className={item.grace_expired ? 'text-red-300' : 'text-warning'}>
                No-Show gemeldet: {teamNameOr(item.no_show_name, 'Team')} nicht erschienen
                {item.grace_expired
                  ? '. Frist abgelaufen'
                  : `. Frist läuft (${item.grace_minutes} min)`}
              </span>
            </div>
          ) : (
            <div className="mt-1.5 text-xs text-foreground/80">
              Ergebnis gemeldet:{' '}
              <span className="font-medium text-foreground">
                {teamNameOr(item.winner_name, 'Sieger')}
              </span>{' '}
              gewinnt
              {item.deadlock_match_id ? (
                <span className="ml-1 text-muted">
                  · Match-ID <code className="text-foreground/70">{item.deadlock_match_id}</code>
                </span>
              ) : null}
            </div>
          )}
        </div>

        <div className="flex shrink-0 gap-1.5">
          <Button
            variant="primary"
            size="sm"
            disabled={busy}
            onClick={() => confirm.mutate(item.report_id)}
          >
            <Check size={13} />
            {item.is_no_show ? 'Walkover' : 'Bestätigen'}
          </Button>
          <Button
            variant="ghost"
            size="sm"
            disabled={busy}
            onClick={() => reject.mutate(item.report_id)}
          >
            <X size={13} />
            Verwerfen
          </Button>
        </div>
      </div>
      {actionError && (
        <p className="mt-2 text-xs text-red-400">
          {actionError instanceof Error && actionError.message ? actionError.message : 'Aktion fehlgeschlagen'}
        </p>
      )}
    </div>
  )
}

export default function Leitstand({ tournamentId }: LeitstandProps) {
  const { data, isLoading, error } = useActionItems(tournamentId)
  const items = data?.pending_reports ?? []

  if (isLoading || (data === undefined && !error)) return null

  if (data === undefined) {
    return (
      <Card className="space-y-2 p-4">
        <div className="flex items-center gap-2">
          <AlertTriangle size={16} className="text-red-400" />
          <h2 className="text-sm font-semibold uppercase tracking-wider text-foreground">Gemeldete Ergebnisse</h2>
        </div>
        <p role="alert" className="text-xs text-red-400">
          Gemeldete Ergebnisse konnten nicht geladen werden{error instanceof Error && error.message ? `: ${error.message}` : '.'}
        </p>
      </Card>
    )
  }

  return (
    <Card className="space-y-3 p-4">
      <div className="flex items-center gap-2">
        {items.length > 0 ? (
          <AlertTriangle size={16} className="text-warning" />
        ) : (
          <CheckCircle2 size={16} className="text-green-400" />
        )}
        <h2 className="text-sm font-semibold uppercase tracking-wider text-foreground">
          {items.length > 0 ? 'Aktion erforderlich' : 'Gemeldete Ergebnisse'}
        </h2>
        {items.length > 0 && (
          <span className="rounded-full bg-warning/20 px-2 py-0.5 text-[10px] font-medium text-warning">
            {items.length}
          </span>
        )}
      </div>

      {error && (
        <p role="alert" className="text-xs text-red-400">
          Aktualisierung fehlgeschlagen, die Liste zeigt den letzten geladenen Stand.
        </p>
      )}

      {items.length === 0 ? (
        <p className="text-xs text-muted">
          Nichts zu tun. Es liegen keine gemeldeten Ergebnisse oder No-Shows vor.
        </p>
      ) : (
        <div className="space-y-2">
          {items.map((item) => (
            <ActionRow key={item.report_id} item={item} tournamentId={tournamentId} />
          ))}
        </div>
      )}
    </Card>
  )
}
