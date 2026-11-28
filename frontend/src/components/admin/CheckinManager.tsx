import { useMemo, useState } from 'react'
import ParticipantMarks from '@/components/admin/ParticipantMarks'
import CheckinStartPlan from '@/components/admin/CheckinStartPlan'
import Card from '@/components/ui/Card'
import Button from '@/components/ui/Button'
import { useAdminCheckin, useAdminCheckins, useDeleteAdminSignup, useDeleteAdminTeam, useFinalizeCheckin, useRevertCheckin } from '@/hooks/useTournament'
import { usePendingDraftChanges } from '@/components/admin/teamBuilderDrafts'
import { latestActionError } from '@/lib/latestError'
import { keepsRosters } from '@/lib/checkinMode'
import { bracketFormatLabel } from '@/lib/bracketFormat'
import { pluralWord, pluralize } from '@/lib/countLabel'
import type {
  FinalizeCheckinResult,
  Team,
  TournamentSignup,
} from '@/types/tournament'
import { AlertCircle, CheckCircle2, ClipboardCheck, TriangleAlert, UserCheck, X } from 'lucide-react'
import { playerName } from '@/lib/playerName'
import { teamNameOr } from '@/lib/teamName'

interface CheckinManagerProps {
  tournamentId: number
  teamSize: number
  teams: Team[]
  signups: TournamentSignup[]
  league?: boolean
  onStatusChanged?: (message: string) => void
}

interface ParticipantRow {
  discord_id: string
  discord_name: string | null
  team_id: number | null
  team_name: string
  signup_id: number | null
}

export default function CheckinManager({
  tournamentId,
  teamSize,
  teams,
  signups,
  league = false,
  onStatusChanged,
}: CheckinManagerProps) {
  const { data: adminCheckins, error: checkinsError } = useAdminCheckins(tournamentId)
  const deleteSignup = useDeleteAdminSignup()
  const adminCheckin = useAdminCheckin(tournamentId)
  const finalizeMutation = useFinalizeCheckin(tournamentId)
  const revertMutation = useRevertCheckin(tournamentId)
  const deleteTeam = useDeleteAdminTeam()

  const [preview, setPreview] = useState<FinalizeCheckinResult | null>(null)
  const [allowedTeams, setAllowedTeams] = useState<Record<number, boolean>>({})
  const [feedback, setFeedback] = useState('')
  const [modalFeedback, setModalFeedback] = useState('')
  const [pendingCheckins, setPendingCheckins] = useState<ReadonlySet<string>>(new Set())

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
        team_name: teamNameOr(team?.name, 'Solo'),
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
      return playerName(left.discord_name).localeCompare(playerName(right.discord_name))
    })
  }, [checkedInIds, signups, teams])

  const error = latestActionError([finalizeMutation, revertMutation, deleteSignup, adminCheckin, deleteTeam]) ?? checkinsError
  const checkinsKnown = adminCheckins !== undefined
  const checkedInCount = participants.filter((row) => checkedInIds.has(row.discord_id)).length
  const isBusy = finalizeMutation.isPending || revertMutation.isPending || deleteSignup.isPending || deleteTeam.isPending
  const modalBusy = isBusy || pendingCheckins.size > 0
  const pendingDrafts = usePendingDraftChanges(tournamentId, preview !== null)
  const pendingBuilderChanges = pendingDrafts.here + pendingDrafts.otherTabs
  const bracketFallback = preview !== null && preview.effective_bracket_format !== preview.bracket_format
  const bracketTooSmall = preview !== null && !preview.bracket_startable
  const openWarnings = preview?.warnings.filter((warning) => !allowedTeams[warning.team_id]).length ?? 0
  const startBlocker = pendingBuilderChanges > 0
    ? 'Erst die Änderungen im Team-Builder speichern.'
    : bracketTooSmall
      ? 'Zu wenige Teams für das Bracket.'
      : openWarnings > 0
        ? `${pluralize(openWarnings, 'unvollständiges Team', 'unvollständige Teams')} noch anhaken oder entfernen.`
        : ''

  const removeParticipant = (row: ParticipantRow) => {
    if (row.signup_id === null) return
    if (!window.confirm(`${playerName(row.discord_name)} aus dem Turnier entfernen? Anmeldung und Check-in werden gelöscht.`)) return
    setFeedback('')
    deleteSignup.mutate(
      { tournamentId, signupId: row.signup_id },
      { onSuccess: () => setFeedback(`${playerName(row.discord_name)} wurde aus dem Turnier entfernt.`) },
    )
  }

  const setCheckinPending = (discordId: string, pending: boolean) => {
    setPendingCheckins((current) => {
      const next = new Set(current)
      if (pending) next.add(discordId)
      else next.delete(discordId)
      return next
    })
  }

  const checkinParticipant = async (row: ParticipantRow) => {
    setFeedback('')
    setCheckinPending(row.discord_id, true)
    try {
      await adminCheckin.mutateAsync(row.discord_id)
      setFeedback(`${playerName(row.discord_name)} ist eingecheckt.`)
    } catch {
      return
    } finally {
      setCheckinPending(row.discord_id, false)
    }
  }

  const runPreview = (fill: boolean) => {
    const previousMode = preview?.auto_team_formation
    setFeedback('')
    setModalFeedback('')
    finalizeMutation.mutate(
      { confirm: false, allowedTeamIds: [], fillFromSolos: fill },
      {
        onSuccess: (result) => {
          const modeChanged = previousMode !== undefined && previousMode !== result.auto_team_formation
          setPreview(result)
          setAllowedTeams((current) =>
            Object.fromEntries(
              result.warnings.map((warning) => [warning.team_id, modeChanged ? false : (current[warning.team_id] ?? false)])
            )
          )
        },
      }
    )
  }

  const removeIncompleteTeam = (teamId: number, teamName: string) => {
    if (!preview) return
    const fill = preview.auto_team_formation
    if (!window.confirm(`${teamName} entfernen? Das Team wird gelöscht, die Spieler landen als Einzelanmeldungen in der Solo-Liste.`)) return
    deleteTeam.mutate(
      { tournamentId, teamId },
      {
        onSuccess: () => {
          runPreview(fill)
          setModalFeedback(`${teamName} wurde entfernt. Die Vorschau ist aktualisiert.`)
        },
      },
    )
  }

  const closePreview = () => {
    setPreview(null)
    setModalFeedback('')
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
        fillFromSolos: preview.auto_team_formation,
      },
      {
        onSuccess: (result) => {
          const message = league
            ? 'Check-in abgeschlossen. Der Spielplan steht, du findest ihn unter „Spielplan“.'
            : result.advanced_to_bracket
              ? 'Check-in abgeschlossen. Das Turnier läuft, die Matches stehen im Bracket.'
              : 'Check-in abgeschlossen. Das Turnier läuft in der Gruppenphase.'
          setFeedback(message)
          onStatusChanged?.(message)
          closePreview()
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
        const message = 'Check-in wurde zurückgesetzt. Das Turnier ist wieder in der Anmeldung.'
        setFeedback(message)
        onStatusChanged?.(message)
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

        {error && !preview && (
          <div role="alert" className="flex items-center gap-2 rounded-lg border border-red-500/20 bg-red-500/10 p-3 text-sm text-red-400">
            <AlertCircle size={16} />
            <span>{error instanceof Error ? error.message : 'Check-in konnte nicht verarbeitet werden'}</span>
          </div>
        )}

        <div className="flex flex-wrap gap-2">
          <Button variant="primary" size="sm" disabled={isBusy} onClick={() => runPreview(false)}>
            {finalizeMutation.isPending ? 'Prüft...' : 'Check-in abschließen & Teams bereinigen'}
          </Button>
        </div>

        <div className="flex flex-col gap-3 border-t border-border pt-4 sm:flex-row sm:items-center sm:justify-between">
          <p className="text-xs text-muted">
            Zurück zur Anmeldung: Alle bisherigen Check-ins werden gelöscht.
          </p>
          <Button
            variant="ghost"
            size="sm"
            className="shrink-0 border border-red-500/40 text-red-400 hover:bg-red-500/10"
            disabled={isBusy}
            onClick={handleRevert}
          >
            {revertMutation.isPending ? 'Setzt zurück...' : 'Check-in zurücksetzen'}
          </Button>
        </div>
      </Card>

      <Card className="p-4 sm:p-6">
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
                  <th className="hidden px-3 py-2 font-medium sm:table-cell">Team</th>
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
                        {playerName(row.discord_name)}
                        <ParticipantMarks discordId={row.discord_id} />
                        <div className="text-xs text-muted sm:hidden">{row.team_name}</div>
                      </td>
                      <td className="hidden px-3 py-2 text-muted sm:table-cell">{row.team_name}</td>
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
                            disabled={isBusy || pendingCheckins.has(row.discord_id)}
                            onClick={() => void checkinParticipant(row)}
                            aria-label={`${playerName(row.discord_name)} einchecken`}
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
                            aria-label={`${playerName(row.discord_name)} aus dem Turnier entfernen`}
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
          </div>
        )}
      </Card>

      {preview && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 px-4">
          <div
            role="dialog"
            aria-modal="true"
            aria-labelledby="checkin-preview-title"
            className="max-h-[90vh] w-full max-w-3xl overflow-auto rounded-xl border border-border bg-card p-4 shadow-xl sm:p-6"
          >
            <div className="flex items-center gap-2">
              <TriangleAlert size={18} className="text-amber-300" />
              <h3 id="checkin-preview-title" className="text-lg font-semibold text-foreground">Check-in Vorschau</h3>
            </div>

            {modalFeedback && (
              <div role="status" className="mt-4 flex items-start gap-2 rounded-xl border border-green-500/20 bg-green-500/10 p-4 text-sm text-green-400">
                <CheckCircle2 size={16} className="mt-0.5 shrink-0" />
                <span>{modalFeedback}</span>
              </div>
            )}

            {pendingBuilderChanges > 0 && (
              <div role="alert" className="mt-4 rounded-xl border border-red-500/30 bg-red-500/10 p-4 text-sm text-red-300">
                Im Team-Builder {pluralWord(pendingBuilderChanges, 'ist', 'sind')} {pluralize(pendingBuilderChanges, 'Änderung', 'Änderungen')} noch nicht gespeichert
                {pendingDrafts.otherTabs > 0 ? `, davon ${pendingDrafts.otherTabs} in einem anderen Tab dieses Browsers` : ''}.
                Speichere sie unter „Teilnehmer“, sonst startet das Turnier ohne sie.
              </div>
            )}

            {preview.tournament_mode_auto && preview.tournament_mode !== 'league' && preview.bracket_startable && (
              <div className="mt-4 rounded-xl border border-border bg-background/60 p-4 text-sm text-foreground">
                Turniermodus Automatisch: Mit {pluralize(preview.final_team_count, 'Team', 'Teams')}{' '}
                {preview.tournament_mode === 'group_stage' ? 'gibt es eine Gruppenphase mit anschließendem Bracket.' : 'geht es direkt ins Bracket.'}{' '}
                Ab {preview.auto_group_stage_threshold} Teams gibt es eine Gruppenphase, darunter nur das Bracket.
              </div>
            )}

            {bracketTooSmall && (
              <div role="alert" className="mt-4 rounded-xl border border-red-500/30 bg-red-500/10 p-4 text-sm text-red-300">
                Für das Bracket fehlen Teams. Nötig sind mindestens {preview.min_bracket_entrants}, gerade {pluralWord(preview.bracket_entrants ?? 0, 'ist', 'sind')} es {preview.bracket_entrants ?? 0}.
                Stell die Teams unter „Teilnehmer“ zusammen und prüfe danach erneut.
              </div>
            )}

            {bracketFallback && !bracketTooSmall && (
              <div role="alert" className="mt-4 rounded-xl border border-amber-500/30 bg-amber-500/10 p-4 text-sm text-amber-200">
                Eingestellt ist {bracketFormatLabel(preview.bracket_format)} Dafür braucht das Bracket {preview.double_elimination_rule}.
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

            {preview.can_fill_from_solos && (
              <label className="mt-5 flex items-start gap-3 rounded-xl border border-border bg-background/60 p-4">
                <input
                  type="checkbox"
                  checked={preview.auto_team_formation}
                  disabled={modalBusy}
                  onChange={(event) => runPreview(event.target.checked)}
                  className="mt-1"
                />
                <span className="text-sm text-foreground">
                  <span className="font-semibold">Teams aus Solo-Spielern auffüllen und neue Teams bilden</span>
                  <span className="mt-1 block text-muted">
                    Aus: Jedes Team startet mit seinem Kader, Solo-Spieler bleiben ohne Team. An: Spieler ohne Check-in
                    fliegen aus ihrem Team, freie Plätze werden mit eingecheckten Solo-Spielern gefüllt und aus dem Rest
                    entstehen neue Teams.
                  </span>
                </span>
              </label>
            )}

            <CheckinStartPlan preview={preview} />

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
                            {playerName(member.discord_name)}
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
                <p className="text-sm text-muted">
                  {keepsRosters(preview)
                    ? 'Hakst du ein Team an, startet es mit seinem Kader, obwohl nicht alle eingecheckt sind.'
                    : 'Hakst du ein Team an, startet es mit den eingecheckten Spielern in Unterzahl.'}{' '}
                  Ohne Haken lässt sich das Turnier nicht starten, bis du das Team anhakst oder entfernst.
                </p>
                {preview.warnings.map((warning) => (
                  <div
                    key={warning.team_id}
                    className="flex flex-col gap-3 rounded-xl border border-amber-500/20 bg-amber-500/10 p-4 sm:flex-row sm:items-start sm:justify-between"
                  >
                    <label className="flex items-start gap-3">
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
                        {keepsRosters(preview)
                          ? `${warning.team_name} trotzdem starten lassen (${warning.current} von ${warning.required} eingecheckt)`
                          : `${warning.team_name} in Unterzahl starten lassen (${warning.current} von ${warning.required} Spielern)`}
                      </span>
                    </label>
                    <Button
                      variant="danger"
                      size="sm"
                      disabled={modalBusy}
                      onClick={() => removeIncompleteTeam(warning.team_id, warning.team_name)}
                    >
                      {deleteTeam.isPending && deleteTeam.variables?.teamId === warning.team_id ? 'Entfernt...' : 'Team entfernen'}
                    </Button>
                  </div>
                ))}
              </div>
            ) : (
              <div className="mt-5 rounded-xl border border-green-500/20 bg-green-500/10 p-4 text-sm text-green-400">
                Keine unvollständigen Teams erkannt.
              </div>
            )}

            {error && (
              <div role="alert" className="mt-6 flex items-start gap-2 rounded-xl border border-red-500/30 bg-red-500/10 p-4 text-sm text-red-300">
                <AlertCircle size={16} className="mt-0.5 shrink-0" />
                <span>{error instanceof Error ? error.message : 'Check-in konnte nicht verarbeitet werden'}</span>
              </div>
            )}

            <div className="mt-6 flex flex-wrap items-center justify-end gap-2">
              {startBlocker && (
                <p id="checkin-start-blocker" className="mr-auto text-sm text-amber-200">{startBlocker}</p>
              )}
              <Button
                variant="ghost"
                size="sm"
                disabled={modalBusy}
                onClick={closePreview}
              >
                Schließen
              </Button>
              <Button
                variant="primary"
                size="sm"
                disabled={modalBusy || startBlocker !== ''}
                aria-describedby={startBlocker ? 'checkin-start-blocker' : undefined}
                onClick={confirmStart}
              >
                {finalizeMutation.isPending ? 'Startet...' : 'Turnier jetzt starten'}
              </Button>
            </div>
          </div>
        </div>
      )}
    </section>
  )
}
