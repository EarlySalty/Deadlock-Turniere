import { useState } from 'react'
import Card from '@/components/ui/Card'
import Button from '@/components/ui/Button'
import { useSoloBalance } from '@/hooks/useTournament'
import type { Team, TournamentStatus } from '@/types/tournament'
import type { BalancePlayer } from '@/types/soloBalance'

interface SoloBalancePanelProps {
  tournamentId: number
  tournamentStatus: TournamentStatus
  teamSize: number
  signupCount: number
  teams: Team[]
}

const formatScore = (value: number) => value.toLocaleString('de-DE', { maximumFractionDigits: 1 })

function PlayerList({ players }: { players: BalancePlayer[] }) {
  return (
    <ul className="divide-y divide-border">
      {players.map((player) => (
        <li key={player.signup_id} className="flex flex-wrap items-center justify-between gap-x-4 gap-y-1 py-2 text-sm">
          <div className="min-w-0">
            <span className="break-words text-foreground">{player.discord_name || 'Unbekannt'}</span>
            {player.priority && <span className="ml-2 text-xs text-primary">Vorrang</span>}
            <p className="text-xs text-muted">{player.used_median ? 'Ohne Rang: Median' : player.rank || 'Mit Rang'}</p>
          </div>
          <span className="shrink-0 tabular-nums text-muted">{formatScore(player.score)}</span>
        </li>
      ))}
    </ul>
  )
}

export default function SoloBalancePanel({ tournamentId, tournamentStatus, teamSize, signupCount, teams }: SoloBalancePanelProps) {
  const { preview, apply, waitlist } = useSoloBalance(tournamentId)
  const [teamCount, setTeamCount] = useState('')
  const maxTeams = Math.floor(signupCount / teamSize)
  const count = teamCount === '' ? maxTeams : Number(teamCount)
  const canBalance = tournamentStatus === 'checkin' && teams.every((team) => team.members.length === 1)
  const busy = preview.isPending || apply.isPending
  const proposal = preview.data
  const error = preview.error || apply.error || waitlist.error
  const validCount = Number.isInteger(count) && count > 0 && count <= maxTeams

  return (
    <Card className="space-y-4 p-6">
      <div>
        <h2 className="text-lg font-semibold text-foreground">Solo-Anmeldungen ausgleichen</h2>
        <p className="mt-1 text-sm text-muted">
          Aktuelle Ränge bestimmen die Teamstärke. Wer auf der letzten Warteliste stand, bekommt Vorrang; danach zählt die Anmeldereihenfolge.
        </p>
      </div>
      {canBalance ? (
        <div className="flex flex-wrap items-end gap-3">
          <label className="space-y-1 text-sm text-muted">
            <span className="block">Teamanzahl (höchstens {maxTeams})</span>
            <input
              type="number" min={1} max={maxTeams} value={teamCount} placeholder={String(maxTeams)}
              disabled={busy}
              onChange={(event) => { setTeamCount(event.target.value); preview.reset(); apply.reset() }}
              className="w-32 rounded-lg border border-border bg-background px-3 py-2 tabular-nums text-foreground focus:outline-none focus:ring-2 focus:ring-primary/50"
            />
          </label>
          <Button disabled={busy || !validCount} onClick={() => { apply.reset(); preview.mutate(count) }}>
            {preview.isPending ? 'Ränge werden geladen…' : 'Teams ausgleichen'}
          </Button>
        </div>
      ) : (
        <p className="text-sm text-muted">
          {tournamentStatus === 'registration' || tournamentStatus === 'draft'
            ? 'Schließe zuerst die Anmeldung und öffne den Check-in.'
            : 'Der Teamausgleich ist während des Check-ins ohne bestehende Mehrpersonen-Teams verfügbar.'}
        </p>
      )}
      {error && <p role="alert" className="text-sm text-red-400">{error.message}</p>}
      {apply.isSuccess && <p role="status" className="text-sm text-green-400">Teams übernommen. Die Warteliste ist gespeichert.</p>}
      {proposal && !apply.isSuccess && (
        <section aria-label="Vorschlag für ausgeglichene Teams" className="space-y-4 border-t border-border pt-4">
          <div>
            <h3 className="font-semibold text-foreground">Vorschlag: {proposal.team_count} Teams mit je {proposal.team_size} Spielern</h3>
            <p className="mt-1 text-sm text-muted">
              Stärkeabstand: <span className="tabular-nums text-foreground">{formatScore(proposal.gap)}</span>. Ohne Rang zählt der Median {formatScore(proposal.median_score)}.
            </p>
            <p className="mt-1 text-xs text-muted">Sind keine Ränge bekannt, werden alle Spieler gleich gewichtet. Dieser Vorschlag wurde noch nicht übernommen.</p>
            {teams.length > 0 && <p className="mt-1 text-sm text-muted">Vorhandene 1-Personen-Teams werden beim Übernehmen ersetzt.</p>}
          </div>
          <div className="grid gap-x-6 gap-y-4 md:grid-cols-2">
            {proposal.teams.map((team) => (
              <section key={team.name} className="min-w-0">
                <h4 className="flex items-center justify-between gap-3 border-b border-border pb-2 font-medium text-foreground">
                  {team.name}<span className="text-sm tabular-nums text-muted">Summe {formatScore(team.score_sum)}</span>
                </h4>
                <PlayerList players={team.players} />
              </section>
            ))}
          </div>
          <section aria-label="Vorgeschlagene Warteliste">
            <h4 className="font-medium text-foreground">Warteliste ({proposal.waitlist.length})</h4>
            {proposal.waitlist.length > 0 ? <PlayerList players={proposal.waitlist} /> : <p className="mt-1 text-sm text-muted">Alle Anmeldungen passen in die Teams.</p>}
          </section>
          <div className="flex flex-wrap items-center gap-3">
            <Button disabled={busy || !canBalance || apply.isError} onClick={() => apply.mutate(proposal)}>
              {apply.isPending ? 'Teams werden übernommen…' : 'Teams übernehmen'}
            </Button>
            <Button variant="ghost" disabled={busy} onClick={() => { preview.reset(); apply.reset() }}>Vorschlag schließen</Button>
          </div>
        </section>
      )}
      {(waitlist.data?.length ?? 0) > 0 && (
        <section aria-label="Gespeicherte Warteliste" className="space-y-2 border-t border-border pt-4">
          <h3 className="font-semibold text-foreground">Warteliste ({waitlist.data?.length})</h3>
          <p className="text-sm text-muted">Diese Spieler erhalten beim nächsten Solo-Teamausgleich Vorrang, wenn sie sich erneut anmelden.</p>
          <ol className="space-y-2 text-sm text-foreground">
            {waitlist.data?.map((player) => <li key={player.signup_id}>{player.position}. {player.discord_name || 'Unbekannt'}</li>)}
          </ol>
        </section>
      )}
    </Card>
  )
}
