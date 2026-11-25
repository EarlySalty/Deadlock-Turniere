import { AlertTriangle, ArrowRight, Check, ClipboardCheck, Flag, GitBranch, Users } from 'lucide-react'
import Button from '@/components/ui/Button'
import { isPugTournament, teamOverview, workflowAction } from '@/lib/adminWorkflow'
import type { AdminPhase } from '@/components/admin/adminPhase'
import type { TournamentDetail } from '@/types/tournament'

interface Props {
  tournament: TournamentDetail
  onNavigate: (phase: AdminPhase) => void
}

const STEPS = [
  { label: 'Anmeldung', icon: Flag },
  { label: 'Teams bilden', icon: Users },
  { label: 'Check-in', icon: ClipboardCheck },
  { label: 'Spiele', icon: GitBranch },
] as const

export default function AdminWorkflowPanel({ tournament, onNavigate }: Props) {
  const action = workflowAction(tournament)
  const counts = teamOverview(tournament)
  const isPug = isPugTournament(tournament)

  return (
    <section aria-label="Turnierablauf" className="space-y-4 rounded-2xl border border-primary/25 bg-card p-4 sm:p-5">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <h2 className="text-lg font-semibold text-foreground">So führst du dieses Turnier durch</h2>
        {isPug && (
          <span className="rounded-md border border-primary/30 bg-primary/10 px-2 py-1 text-xs font-semibold text-primary">
            PUG-Ablauf
          </span>
        )}
      </div>
      <ol className="grid grid-cols-2 gap-2 sm:grid-cols-4" aria-label="Fortschritt">
        {STEPS.map(({ label, icon: Icon }, i) => {
          const done = action.stage > i
          const current = action.stage === i
          return (
            <li
              key={label}
              aria-current={current ? 'step' : undefined}
              className={`flex min-w-0 items-center gap-2 rounded-lg border px-3 py-2.5 text-xs sm:text-sm ${current
                ? 'border-primary/60 bg-primary/10 text-foreground'
                : done ? 'border-border bg-background/40 text-foreground/80' : 'border-border/60 text-muted'}`}
            >
              <span className="flex h-6 w-6 shrink-0 items-center justify-center rounded-full border border-current/20">
                {done ? <Check size={13} /> : <Icon size={13} />}
              </span>
              <span className="min-w-0 font-medium">{label}</span>
            </li>
          )
        })}
      </ol>
      <div className="grid gap-4 md:grid-cols-[minmax(0,1fr)_auto] md:items-center">
        <div className="min-w-0">
          <p className="text-xs font-bold uppercase tracking-wide text-primary">Jetzt wichtig</p>
          <h3 className="mt-1 text-base font-semibold text-foreground">{action.title}</h3>
          <p className="mt-1 text-sm leading-relaxed text-muted">{action.description}</p>
        </div>
        <Button variant="primary" size="sm" onClick={() => onNavigate(action.phase)}>
          {action.button}
          <ArrowRight size={15} />
        </Button>
      </div>
      <div className="flex flex-wrap gap-x-5 gap-y-2 border-t border-border pt-3 text-sm text-muted">
        <span><strong className="text-foreground">{counts.teams}</strong> Teams</span>
        <span><strong className="text-foreground">{counts.playersInTeams}</strong> Spieler in Teams</span>
        <span><strong className="text-foreground">{counts.solos}</strong> Solo-Spieler offen</span>
        <span><strong className="text-foreground">{counts.incompleteTeams}</strong> unvollständige Teams</span>
      </div>
      {isPug && ['draft', 'registration'].includes(tournament.status) && (
        <p className="flex items-start gap-2 border-t border-border pt-3 text-xs leading-relaxed text-amber-300">
          <AlertTriangle size={15} className="mt-0.5 shrink-0" />
          Wichtig: „Keine Einladungen“ verhindert keine selbst gegründeten Teams oder Teambeitritte.
          Eine echte „Nur Solo-Anmeldungen“-Sperre ist derzeit noch nicht umgesetzt.
        </p>
      )}
    </section>
  )
}
