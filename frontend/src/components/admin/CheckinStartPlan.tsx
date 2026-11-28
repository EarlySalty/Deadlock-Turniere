import type { FinalizeCheckinResult, FinalizeCheckinRosterEntry, FinalizeCheckinSoloPlan, FinalizeCheckinTeamPlan } from '@/types/tournament'
import { playerName } from '@/lib/playerName'
import { keepsRosters } from '@/lib/checkinMode'

function teamOutcomeText(plan: FinalizeCheckinTeamPlan, keepRosters: boolean): string {
  if (plan.outcome === 'deleted') return 'Wird gelöscht, niemand hat eingecheckt.'
  if (plan.outcome === 'complete') return keepRosters ? 'Startet unverändert mit diesem Kader.' : 'Startet vollständig.'
  return keepRosters
    ? 'Kader bleibt unverändert. Startet nur mit Haken, obwohl nicht alle eingecheckt sind.'
    : 'Bleibt unvollständig. Startet nur mit Haken in Unterzahl.'
}

function teamOutcomeClass(plan: FinalizeCheckinTeamPlan): string {
  if (plan.outcome === 'deleted') return 'border-red-500/30 bg-red-500/10'
  if (plan.outcome === 'incomplete') return 'border-amber-500/20 bg-amber-500/10'
  return 'border-border bg-background/60'
}

function rosterNote(entry: FinalizeCheckinRosterEntry): string {
  if (entry.change === 'removed') return 'fehlt, fliegt aus dem Team'
  if (entry.change === 'added') return 'kommt aus der Solo-Liste dazu'
  return entry.checked_in ? 'eingecheckt' : 'fehlt, bleibt im Kader'
}

function rosterNoteClass(entry: FinalizeCheckinRosterEntry): string {
  if (entry.change === 'removed') return 'text-red-300'
  if (entry.change === 'added') return 'text-primary'
  return entry.checked_in ? 'text-green-400' : 'text-amber-200'
}

function soloOutcomeText(plan: FinalizeCheckinSoloPlan): string {
  if (plan.outcome === 'joins_team') return `kommt zu ${plan.team_name ?? 'einem Team'}`
  if (plan.outcome === 'new_team') return plan.team_name ? `bildet das neue Team „${plan.team_name}“` : 'bildet ein neues Team'
  return plan.checked_in ? 'bleibt ohne Team, spielt nicht mit' : 'nicht eingecheckt, bleibt ohne Team'
}

function TeamPlanCard({ plan, keepRosters }: { plan: FinalizeCheckinTeamPlan; keepRosters: boolean }) {
  return (
    <li className={`rounded-xl border p-3 ${teamOutcomeClass(plan)}`}>
      <div className="flex flex-wrap items-baseline justify-between gap-x-3 gap-y-1">
        <span className="font-semibold text-foreground">{plan.team_name}</span>
        <span className="text-xs text-muted">
          {plan.checked_in} von {plan.required} eingecheckt, {plan.roster.length} im Kader
        </span>
      </div>
      <p className="mt-1 text-sm text-foreground">{teamOutcomeText(plan, keepRosters)}</p>
      <ul className="mt-2 space-y-0.5 text-sm">
        {plan.roster.map((entry) => (
          <li key={entry.discord_id} className="flex flex-wrap justify-between gap-x-3">
            <span className="text-foreground">{playerName(entry.discord_name)}</span>
            <span className={`text-xs ${rosterNoteClass(entry)}`}>{rosterNote(entry)}</span>
          </li>
        ))}
      </ul>
    </li>
  )
}

export default function CheckinStartPlan({ preview }: { preview: FinalizeCheckinResult }) {
  const keepRosters = keepsRosters(preview)
  return (
    <>
      {preview.team_plans.length > 0 && (
        <div className="mt-5 space-y-2">
          <h4 className="text-base font-semibold text-foreground">Das passiert mit jedem Team</h4>
          <ul className="space-y-2">
            {preview.team_plans.map((plan) => (
              <TeamPlanCard key={plan.team_id} plan={plan} keepRosters={keepRosters} />
            ))}
          </ul>
        </div>
      )}

      {preview.solo_plans.length > 0 && (
        <div className="mt-5 space-y-2">
          <h4 className="text-base font-semibold text-foreground">Das passiert mit jedem Solo-Spieler</h4>
          {!preview.auto_team_formation && (
            <p className="text-sm text-muted">
              {preview.can_fill_from_solos
                ? 'Auffüllen aus der Solo-Liste ist aus. Solo-Spieler kommen in kein Team.'
                : 'Im PUG-Modus bildet das Tool keine Teams automatisch. Baue sie vorher im Team-Builder.'}
            </p>
          )}
          <ul className="space-y-0.5 rounded-xl border border-border bg-background/60 p-3 text-sm">
            {preview.solo_plans.map((plan) => (
              <li key={plan.discord_id} className="flex flex-wrap justify-between gap-x-3">
                <span className="text-foreground">{playerName(plan.discord_name)}</span>
                <span className={`text-xs ${plan.outcome === 'stays_solo' ? 'text-muted' : 'text-primary'}`}>
                  {soloOutcomeText(plan)}
                </span>
              </li>
            ))}
          </ul>
        </div>
      )}
    </>
  )
}
