import { useMemo, useState } from 'react'
import Card from '@/components/ui/Card'
import Button from '@/components/ui/Button'
import { useCheckinStatus, useFinalizeCheckin } from '@/hooks/useTournament'
import type {
  FinalizeCheckinResult,
  Team,
  TournamentSignup,
} from '@/types/tournament'
import { AlertCircle, CheckCircle2, ClipboardCheck, TriangleAlert } from 'lucide-react'

interface CheckinManagerProps {
  tournamentId: number
  teamSize: number
  teams: Team[]
  signups: TournamentSignup[]
}

interface ParticipantRow {
  discord_id: string
  discord_name: string | null
  team_id: number | null
  team_name: string
}

function participantName(row: ParticipantRow): string {
  return row.discord_name?.trim() || row.discord_id
}

export default function CheckinManager({
  tournamentId,
  teamSize,
  teams,
  signups,
}: CheckinManagerProps) {
  const { data: checkinStatus } = useCheckinStatus(tournamentId)
  const finalizeMutation = useFinalizeCheckin(tournamentId)

  const [preview, setPreview] = useState<FinalizeCheckinResult | null>(null)
  const [allowedTeams, setAllowedTeams] = useState<Record<number, boolean>>({})
  const [feedback, setFeedback] = useState('')

  const checkedInNames = new Set(checkinStatus?.checked_in_names ?? [])

  const participants = useMemo(() => {
    const rows = new Map<string, ParticipantRow>()

    for (const signup of signups) {
      const team = signup.team_id ? teams.find((entry) => entry.id === signup.team_id) : null
      rows.set(signup.discord_id, {
        discord_id: signup.discord_id,
        discord_name: signup.discord_name,
        team_id: signup.team_id,
        team_name: team?.name ?? 'Solo',
      })
    }

    for (const team of teams) {
      for (const member of team.members) {
        rows.set(member.discord_id, {
          discord_id: member.discord_id,
          discord_name: member.discord_name,
          team_id: team.id,
          team_name: team.name,
        })
      }
    }

    return Array.from(rows.values()).sort((left, right) => {
      const leftMissing = checkedInNames.has(left.discord_name?.trim() || 'Unbekannt') ? 1 : 0
      const rightMissing = checkedInNames.has(right.discord_name?.trim() || 'Unbekannt') ? 1 : 0
      if (leftMissing !== rightMissing) return rightMissing - leftMissing
      if (left.team_name !== right.team_name) return left.team_name.localeCompare(right.team_name)
      return participantName(left).localeCompare(participantName(right))
    })
  }, [checkedInNames, signups, teams])

  const error = finalizeMutation.error
  const isBusy = finalizeMutation.isPending

  const runPreview = () => {
    setFeedback('')
    finalizeMutation.mutate(
      { confirm: false, allowedTeamIds: [] },
      {
        onSuccess: (result) => {
          setPreview(result)
          setAllowedTeams(
            Object.fromEntries(result.warnings.map((warning) => [warning.team_id, false]))
          )
        },
      }
    )
  }

  const confirmStart = () => {
    if (!preview) return
    const allowedTeamIds = Object.entries(allowedTeams)
      .filter(([, isAllowed]) => isAllowed)
      .map(([teamId]) => Number(teamId))

    finalizeMutation.mutate(
      {
        confirm: true,
        allowedTeamIds,
        snapshotToken: preview.snapshot_token,
      },
      {
        onSuccess: () => {
          setFeedback('Check-in abgeschlossen und Turnier in die Gruppenphase verschoben.')
          setPreview(null)
        },
      }
    )
  }

  return (
    <section className="space-y-4">
      <Card className="p-6 space-y-4">
        <div className="flex items-center gap-2">
          <ClipboardCheck size={18} className="text-primary" />
          <h2 className="text-lg font-semibold text-foreground">Check-in verwalten</h2>
        </div>

        <div className="grid gap-3 md:grid-cols-3">
          <div className="rounded-xl border border-border bg-background/60 p-4">
            <div className="text-sm text-muted">Registriert</div>
            <div className="mt-1 text-2xl font-semibold text-foreground">
              {checkinStatus?.total_registered ?? participants.length}
            </div>
          </div>
          <div className="rounded-xl border border-border bg-background/60 p-4">
            <div className="text-sm text-muted">Eingecheckt</div>
            <div className="mt-1 text-2xl font-semibold text-green-400">
              {checkinStatus?.total_checked_in ?? checkedInNames.size}
            </div>
          </div>
          <div className="rounded-xl border border-border bg-background/60 p-4">
            <div className="text-sm text-muted">Teamgröße</div>
            <div className="mt-1 text-2xl font-semibold text-foreground">{teamSize}</div>
          </div>
        </div>

        {feedback && (
          <div className="flex items-center gap-2 rounded-lg border border-green-500/20 bg-green-500/10 p-3 text-sm text-green-400">
            <CheckCircle2 size={16} />
            <span>{feedback}</span>
          </div>
        )}

        {error && (
          <div className="flex items-center gap-2 rounded-lg border border-red-500/20 bg-red-500/10 p-3 text-sm text-red-400">
            <AlertCircle size={16} />
            <span>{error instanceof Error ? error.message : 'Check-in konnte nicht verarbeitet werden'}</span>
          </div>
        )}

        <Button variant="primary" size="sm" disabled={isBusy} onClick={runPreview}>
          {finalizeMutation.isPending ? 'Prüft...' : 'Check-in abschließen & Teams bereinigen'}
        </Button>
      </Card>

      <Card className="p-6">
        <div className="mb-4">
          <h3 className="text-base font-semibold text-foreground">Teilnehmerstatus</h3>
          <p className="mt-1 text-sm text-muted">Fehlende Spieler werden zuerst angezeigt.</p>
        </div>

        {participants.length === 0 ? (
          <p className="text-sm text-muted">Keine registrierten Spieler gefunden.</p>
        ) : (
          <div className="overflow-x-auto">
            <table className="min-w-full text-sm">
              <thead>
                <tr className="border-b border-border text-left text-muted">
                  <th className="px-3 py-2 font-medium">Spieler</th>
                  <th className="px-3 py-2 font-medium">Team</th>
                  <th className="px-3 py-2 font-medium">Status</th>
                </tr>
              </thead>
              <tbody>
                {participants.map((row) => {
                  const checkedIn = checkedInNames.has(row.discord_name?.trim() || 'Unbekannt')
                  return (
                    <tr key={row.discord_id} className="border-b border-border/60">
                      <td className="px-3 py-2 text-foreground">{participantName(row)}</td>
                      <td className="px-3 py-2 text-muted">{row.team_name}</td>
                      <td className="px-3 py-2">
                        <span
                          className={`inline-flex rounded-full px-2.5 py-0.5 text-xs font-medium ${
                            checkedIn
                              ? 'bg-green-500/20 text-green-400'
                              : 'bg-red-500/15 text-red-300'
                          }`}
                        >
                          {checkedIn ? 'eingecheckt' : 'fehlt'}
                        </span>
                      </td>
                    </tr>
                  )
                })}
              </tbody>
            </table>
          </div>
        )}
      </Card>

      {preview && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 px-4">
          <div className="max-h-[90vh] w-full max-w-3xl overflow-auto rounded-xl border border-border bg-card p-6 shadow-xl">
            <div className="flex items-center gap-2">
              <TriangleAlert size={18} className="text-amber-300" />
              <h3 className="text-lg font-semibold text-foreground">Check-in Vorschau</h3>
            </div>

            <div className="mt-4 grid gap-3 md:grid-cols-2">
              <div className="rounded-xl border border-border bg-background/60 p-4">
                <div className="text-sm text-muted">Entfernte Spieler</div>
                <div className="mt-1 text-2xl font-semibold text-foreground">{preview.removed_players.length}</div>
              </div>
              <div className="rounded-xl border border-border bg-background/60 p-4">
                <div className="text-sm text-muted">Hinzugefügte Spieler</div>
                <div className="mt-1 text-2xl font-semibold text-foreground">{preview.added_players.length}</div>
              </div>
            </div>

            {preview.warnings.length > 0 ? (
              <div className="mt-5 space-y-3">
                <h4 className="text-base font-semibold text-foreground">Unvollständige Teams</h4>
                {preview.warnings.map((warning) => (
                  <label
                    key={warning.team_id}
                    className="flex items-start gap-3 rounded-xl border border-amber-500/20 bg-amber-500/10 p-4"
                  >
                    <input
                      type="checkbox"
                      checked={allowedTeams[warning.team_id] ?? false}
                      onChange={(event) =>
                        setAllowedTeams((current) => ({
                          ...current,
                          [warning.team_id]: event.target.checked,
                        }))
                      }
                      className="mt-1"
                    />
                    <span className="text-sm text-foreground">
                      {warning.team_name} hat {warning.current}/{warning.required} Spieler und darf trotzdem starten.
                    </span>
                  </label>
                ))}
              </div>
            ) : (
              <div className="mt-5 rounded-xl border border-green-500/20 bg-green-500/10 p-4 text-sm text-green-400">
                Keine unvollständigen Teams erkannt.
              </div>
            )}

            {preview.remaining_solo_players.length > 0 && (
              <div className="mt-5 rounded-xl border border-border bg-background/60 p-4">
                <h4 className="text-base font-semibold text-foreground">Übrig gebliebene Solo-Spieler</h4>
                <p className="mt-1 text-sm text-muted">
                  Diese Spieler konnten nicht mehr zu einem vollständigen Team zusammengefasst werden.
                </p>
                <div className="mt-3 flex flex-wrap gap-2">
                  {preview.remaining_solo_players.map((player) => (
                    <span
                      key={player.discord_id}
                      className="rounded-full bg-card px-3 py-1 text-xs text-foreground"
                    >
                      {player.discord_name?.trim() || player.discord_id}
                    </span>
                  ))}
                </div>
              </div>
            )}

            <div className="mt-6 flex justify-end gap-2">
              <Button
                variant="ghost"
                size="sm"
                disabled={isBusy}
                onClick={() => setPreview(null)}
              >
                Schließen
              </Button>
              <Button variant="primary" size="sm" disabled={isBusy} onClick={confirmStart}>
                {finalizeMutation.isPending ? 'Startet...' : 'Turnier jetzt starten'}
              </Button>
            </div>
          </div>
        </div>
      )}
    </section>
  )
}
