import { useMemo, useState } from 'react'
import ParticipantMarks from '@/components/admin/ParticipantMarks'
import { DuplicateNamesContext } from '@/components/admin/duplicateNamesContext'
import { duplicateNameKeys } from '@/lib/participantName'
import Card from '@/components/ui/Card'
import Button from '@/components/ui/Button'
import { useAdminCheckin, useAdminCheckins, useDeleteAdminSignup, useFinalizeCheckin, useRevertCheckin } from '@/hooks/useTournament'
import { pendingDraftChanges } from '@/components/admin/teamBuilderDrafts'
import { bracketFormatLabel, DOUBLE_ELIMINATION_RULE } from '@/lib/bracketFormat'
import { pluralize } from '@/lib/countLabel'
import type {
  FinalizeCheckinResult,
  Team,
  TournamentSignup,
} from '@/types/tournament'
import { AlertCircle, CheckCircle2, ClipboardCheck, TriangleAlert, UserCheck, X } from 'lucide-react'

interface CheckinManagerProps {
  tournamentId: number
  teamSize: number
  teams: Team[]
  signups: TournamentSignup[]
  league?: boolean
}

interface ParticipantRow {
  discord_id: string
  discord_name: string | null
  team_id: number | null
  team_name: string
  signup_id: number | null
}

function participantName(player: { discord_name: string | null }): string {
  return player.discord_name?.trim() || 'Unbekannt'
}

export default function CheckinManager({
  tournamentId,
  teamSize,
  teams,
  signups,
  league = false,
}: CheckinManagerProps) {
  const { data: adminCheckins, error: checkinsError } = useAdminCheckins(tournamentId)
  const deleteSignup = useDeleteAdminSignup()
  const adminCheckin = useAdminCheckin(tournamentId)
  const finalizeMutation = useFinalizeCheckin(tournamentId)
  const revertMutation = useRevertCheckin(tournamentId)

  const [preview, setPreview] = useState<FinalizeCheckinResult | null>(null)
  const [allowedTeams, setAllowedTeams] = useState<Record<number, boolean>>({})
  const [feedback, setFeedback] = useState('')

  const checkedInIds = useMemo(
    () => new Set((adminCheckins ?? []).map((entry) => entry.discord_id)),
    [adminCheckins],
  )

  const participants = useMemo(() => {
    const rows = new Map<string, ParticipantRow>()

    for (const signup of signups) {
      const team = signup.team_id ? teams.find((entry) => entry.id === signup.team_id) : null
      rows.set(signup.discord_id, {
        discord_id: signup.discord_id,
        discord_name: signup.discord_name,
        team_id: signup.team_id,
        team_name: team?.name ?? 'Solo',
        signup_id: signup.team_id === null ? signup.id : null,
      })
    }

    for (const team of teams) {
      for (const member of team.members) {
        rows.set(member.discord_id, {
          discord_id: member.discord_id,
          discord_name: member.discord_name,
          team_id: team.id,
          team_name: team.name,
          signup_id: null,
        })
      }
    }

    return Array.from(rows.values()).sort((left, right) => {
      const leftMissing = checkedInIds.has(left.discord_id) ? 0 : 1
      const rightMissing = checkedInIds.has(right.discord_id) ? 0 : 1
      if (leftMissing !== rightMissing) return rightMissing - leftMissing
      if (left.team_name !== right.team_name) return left.team_name.localeCompare(right.team_name)
      return participantName(left).localeCompare(participantName(right))
    })
  }, [checkedInIds, signups, teams])

  const duplicateNames = useMemo(
    () => duplicateNameKeys(participants.map((row) => row.discord_name)),
    [participants],
  )

  const error = finalizeMutation.error ?? revertMutation.error ?? deleteSignup.error ?? adminCheckin.error ?? checkinsError
  const checkinsKnown = adminCheckins !== undefined
  const checkedInCount = participants.filter((row) => checkedInIds.has(row.discord_id)).length
  const isBusy = finalizeMutation.isPending || revertMutation.isPending || deleteSignup.isPending || adminCheckin.isPending
  const pendingDrafts = preview ? pendingDraftChanges(tournamentId) : { here: 0, otherTabs: 0 }
  const pendingBuilderChanges = pendingDrafts.here + pendingDrafts.otherTabs
  const bracketFallback = preview !== null && preview.effective_bracket_format !== preview.bracket_format
  const bracketTooSmall = preview !== null && preview.bracket_entrants !== null && preview.bracket_entrants < 2

  const removeParticipant = (row: ParticipantRow) => {
    if (row.signup_id === null) return
    if (!window.confirm(`${participantName(row)} aus dem Turnier entfernen? Anmeldung und Check-in werden gelöscht.`)) return
    setFeedback('')
    deleteSignup.mutate(
      { tournamentId, signupId: row.signup_id },
      { onSuccess: () => setFeedback(`${participantName(row)} wurde aus dem Turnier entfernt.`) },
    )
  }

  const checkinParticipant = (row: ParticipantRow) => {
    setFeedback('')
    adminCheckin.mutate(row.discord_id, {
      onSuccess: () => setFeedback(`${participantName(row)} ist eingecheckt.`),
    })
  }

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
        onSuccess: (result) => {
          setFeedback(
            league
              ? 'Check-in abgeschlossen. Der Spielplan steht, du findest ihn unter „Spielplan“.'
              : result.advanced_to_bracket
                ? 'Check-in abgeschlossen und Turnier direkt ins Bracket verschoben.'
                : 'Check-in abgeschlossen und Turnier in die Gruppenphase verschoben.'
          )
          setPreview(null)
        },
      }
    )
  }

  const handleRevert = () => {
    if (!window.confirm('Check-in zurück auf Anmeldung setzen und alle Check-ins löschen?')) return

    setFeedback('')
    revertMutation.mutate(undefined, {
      onSuccess: () => {
        setPreview(null)
        setAllowedTeams({})
        setFeedback('Check-in wurde zurückgesetzt. Das Turnier ist wieder in der Anmeldung.')
      },
    })
  }

  return (
    <section className="space-y-4">
      <Card className="p-6 space-y-4">
        <div className="flex items-center gap-2">
          <ClipboardCheck size={18} className="text-primary" />
          <h2 className="text-lg font-semibold text-foreground">Check-in verwalten</h2>
        </div>
        <p className="text-sm text-muted">
          Der Check-in endet nicht von selbst. Wenn alle da sind, schließt du ihn hier mit „Check-in abschließen“ ab.
        </p>

        <div className="grid gap-3 md:grid-cols-3">
          <div className="rounded-xl border border-border bg-background/60 p-4">
            <div className="text-sm text-muted">Registriert</div>
            <div className="mt-1 text-2xl font-semibold text-foreground">
              {participants.length}
            </div>
          </div>
          <div className="rounded-xl border border-border bg-background/60 p-4">
            <div className="text-sm text-muted">Eingecheckt</div>
            <div className="mt-1 text-2xl font-semibold text-green-400">
              {checkinsKnown ? checkedInCount : '…'}
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

        <div className="flex flex-wrap gap-2">
          <Button variant="primary" size="sm" disabled={isBusy} onClick={runPreview}>
            {finalizeMutation.isPending ? 'Prüft...' : 'Check-in abschließen & Teams bereinigen'}
          </Button>
          <Button variant="danger" size="sm" disabled={isBusy} onClick={handleRevert}>
            {revertMutation.isPending ? 'Setzt zurück...' : 'Check-in zurücksetzen'}
          </Button>
        </div>
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
            <DuplicateNamesContext.Provider value={duplicateNames}>
            <table className="min-w-full text-sm">
              <thead>
                <tr className="border-b border-border text-left text-muted">
                  <th className="px-3 py-2 font-medium">Spieler</th>
                  <th className="px-3 py-2 font-medium">Team</th>
                  <th className="px-3 py-2 font-medium">Status</th>
                  <th className="px-3 py-2 font-medium"><span className="sr-only">Aktion</span></th>
                </tr>
              </thead>
              <tbody>
                {participants.map((row) => {
                  const checkedIn = checkedInIds.has(row.discord_id)
                  return (
                    <tr key={row.discord_id} className="border-b border-border/60">
                      <td className="px-3 py-2 text-foreground">
                        {participantName(row)}
                        <ParticipantMarks discordId={row.discord_id} name={row.discord_name} />
                      </td>
                      <td className="px-3 py-2 text-muted">{row.team_name}</td>
                      <td className="px-3 py-2">
                        <span
                          className={`inline-flex rounded-full px-2.5 py-0.5 text-xs font-medium ${
                            !checkinsKnown
                              ? 'bg-card text-muted'
                              : checkedIn
                                ? 'bg-green-500/20 text-green-400'
                                : 'bg-red-500/15 text-red-300'
                          }`}
                        >
                          {!checkinsKnown ? 'unbekannt' : checkedIn ? 'eingecheckt' : 'fehlt'}
                        </span>
                      </td>
                      <td className="px-3 py-2 text-right whitespace-nowrap">
                        {checkinsKnown && !checkedIn && (
                          <button
                            type="button"
                            disabled={isBusy}
                            onClick={() => checkinParticipant(row)}
                            aria-label={`${participantName(row)} einchecken`}
                            title="Einchecken"
                            className="rounded p-1.5 text-muted hover:bg-green-500/10 hover:text-green-400 disabled:opacity-50"
                          >
                            <UserCheck size={15} />
                          </button>
                        )}
                        {row.signup_id !== null && (
                          <button
                            type="button"
                            disabled={isBusy}
                            onClick={() => removeParticipant(row)}
                            aria-label={`${participantName(row)} aus dem Turnier entfernen`}
                            title="Aus dem Turnier entfernen"
                            className="rounded p-1.5 text-muted hover:bg-red-500/10 hover:text-red-300 disabled:opacity-50"
                          >
                            <X size={15} />
                          </button>
                        )}
                      </td>
                    </tr>
                  )
                })}
              </tbody>
            </table>
            </DuplicateNamesContext.Provider>
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

            {pendingBuilderChanges > 0 && (
              <div role="alert" className="mt-4 rounded-xl border border-red-500/30 bg-red-500/10 p-4 text-sm text-red-300">
                Im Team-Builder {pendingBuilderChanges === 1 ? 'ist 1 Änderung' : `sind ${pendingBuilderChanges} Änderungen`} noch nicht gespeichert
                {pendingDrafts.otherTabs > 0 ? `, davon ${pendingDrafts.otherTabs} in einem anderen Tab dieses Browsers` : ''}.
                Speichere sie unter „Teilnehmer“, sonst startet das Turnier ohne sie.
              </div>
            )}

            {bracketTooSmall && (
              <div role="alert" className="mt-4 rounded-xl border border-red-500/30 bg-red-500/10 p-4 text-sm text-red-300">
                Für das Bracket fehlen Teams. Nötig sind mindestens 2, gerade {preview.bracket_entrants === 1 ? 'ist es 1' : `sind es ${preview.bracket_entrants}`}.
                Stell die Teams unter „Teilnehmer“ zusammen und prüfe danach erneut.
              </div>
            )}

            {bracketFallback && !bracketTooSmall && (
              <div role="alert" className="mt-4 rounded-xl border border-amber-500/30 bg-amber-500/10 p-4 text-sm text-amber-200">
                Eingestellt ist {bracketFormatLabel(preview.bracket_format)} Dafür braucht das Bracket {DOUBLE_ELIMINATION_RULE}.
                Mit {pluralize(preview.bracket_entrants ?? 0, 'Team', 'Teams')} im Bracket wird diesmal {bracketFormatLabel(preview.effective_bracket_format)} gespielt.
                Die Einstellung des Turniers bleibt.
              </div>
            )}

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

            {preview.removed_players.length > 0 && (
              <div className="mt-5 space-y-2">
                <h4 className="text-base font-semibold text-foreground">Ohne Check-in, fliegen aus ihrem Team</h4>
                <ul className="space-y-1 text-sm text-foreground">
                  {preview.removed_players.map((player) => (
                    <li key={`${player.team_id}-${player.discord_id}`}>
                      {participantName(player)} <span className="text-muted">aus {player.team_name}</span>
                    </li>
                  ))}
                </ul>
              </div>
            )}

            {preview.added_players.some((player) => player.source === 'solo_pool') && (
              <div className="mt-5 space-y-2">
                <h4 className="text-base font-semibold text-foreground">Auffüllen aus der Solo-Liste</h4>
                <ul className="space-y-1 text-sm text-foreground">
                  {preview.added_players
                    .filter((player) => player.source === 'solo_pool')
                    .map((player) => (
                      <li key={`${player.team_id}-${player.discord_id}`}>
                        {participantName(player)} <span className="text-muted">kommt zu {player.team_name}</span>
                      </li>
                    ))}
                </ul>
              </div>
            )}

            {preview.created_teams.length > 0 && (
              <div className="mt-5 space-y-3">
                <h4 className="text-base font-semibold text-foreground">Automatisch gebildete Teams</h4>
                <p className="text-sm text-amber-200">
                  Diese Teams entstehen aus der Reihenfolge des Check-ins, nicht aus einer Wahl. Wenn ihr eigene Teams habt,
                  baut sie vorher im Team-Builder und speichert sie.
                </p>
                <div className="grid gap-3 md:grid-cols-2">
                  {preview.created_teams.map((team) => (
                    <div key={team.team_name} className="rounded-xl border border-border bg-background/60 p-3">
                      <div className="font-semibold text-foreground">{team.team_name}</div>
                      <ul className="mt-2 space-y-0.5 text-sm text-foreground">
                        {team.members.map((member, index) => (
                          <li key={member.discord_id}>
                            {participantName(member)}
                            {index === 0 && <span className="ml-1 text-xs text-primary">Captain</span>}
                          </li>
                        ))}
                      </ul>
                    </div>
                  ))}
                </div>
              </div>
            )}

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
                  {preview.auto_team_formation
                    ? 'Diese Spieler konnten nicht mehr zu einem vollständigen Team zusammengefasst werden und spielen nicht mit.'
                    : 'Diese Spieler sind in keinem Team und spielen nicht mit. Im PUG-Modus bildet das Tool keine Teams automatisch.'}
                </p>
                <div className="mt-3 flex flex-wrap gap-2">
                  {preview.remaining_solo_players.map((player) => (
                    <span
                      key={player.discord_id}
                      className="rounded-full bg-card px-3 py-1 text-xs text-foreground"
                    >
                      {participantName(player)}
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
              <Button variant="primary" size="sm" disabled={isBusy || pendingBuilderChanges > 0 || bracketTooSmall} onClick={confirmStart}>
                {finalizeMutation.isPending ? 'Startet...' : 'Turnier jetzt starten'}
              </Button>
            </div>
          </div>
        </div>
      )}
    </section>
  )
}
