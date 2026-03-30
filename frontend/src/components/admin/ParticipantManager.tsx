import { useMemo, useState } from 'react'
import Card from '@/components/ui/Card'
import Button from '@/components/ui/Button'
import { useAuth } from '@/hooks/useAuth'
import {
  useAddMember,
  useAssignSignupToAdminTeam,
  useChangeAdminCaptain,
  useCreateAdminTeam,
  useDeleteAdminSignup,
  useDeleteAdminTeam,
  useMoveAdminTeamMember,
  useRemoveAdminTeamMember,
  useRenameAdminTeam,
} from '@/hooks/useTournament'
import type { Team, TeamMember, TournamentSignup, TournamentStatus } from '@/types/tournament'
import {
  AlertCircle,
  ArrowRightLeft,
  Crown,
  Plus,
  Trash2,
  UserMinus,
  UserPlus,
  Users,
} from 'lucide-react'

interface ParticipantManagerProps {
  tournamentId: number
  tournamentStatus: TournamentStatus
  teamSize: number
  teams: Team[]
  signups: TournamentSignup[]
}

function playerLabel(member: Pick<TeamMember, 'discord_id' | 'discord_name'>): string {
  return member.discord_name ?? member.discord_id
}

function captainLabel(team: Team): string {
  const captain = team.members.find((member) => member.discord_id === team.captain_discord_id)
  return captain ? playerLabel(captain) : team.captain_discord_id || 'noch keiner gesetzt'
}

function signupName(signup: TournamentSignup): string {
  return signup.discord_name?.trim() || signup.discord_id
}

export default function ParticipantManager({
  tournamentId,
  tournamentStatus,
  teamSize,
  teams,
  signups,
}: ParticipantManagerProps) {
  const { user } = useAuth()
  const addMemberMutation = useAddMember(tournamentId)
  const createTeamMutation = useCreateAdminTeam()
  const renameTeamMutation = useRenameAdminTeam()
  const deleteTeamMutation = useDeleteAdminTeam()
  const changeCaptainMutation = useChangeAdminCaptain()
  const removeMemberMutation = useRemoveAdminTeamMember()
  const moveMemberMutation = useMoveAdminTeamMember()
  const assignSignupMutation = useAssignSignupToAdminTeam()
  const deleteSignupMutation = useDeleteAdminSignup()

  const [newTeamName, setNewTeamName] = useState('')
  const [renameValues, setRenameValues] = useState<Record<number, string>>({})
  const [moveTargets, setMoveTargets] = useState<Record<string, string>>({})
  const [signupTargets, setSignupTargets] = useState<Record<number, string>>({})
  const [replacementTeam, setReplacementTeam] = useState<Team | null>(null)
  const [replacementDiscordId, setReplacementDiscordId] = useState('')
  const [replacementDiscordName, setReplacementDiscordName] = useState('')
  const [feedback, setFeedback] = useState('')

  const pendingSignups = useMemo(
    () => signups.filter((signup) => signup.team_id === null),
    [signups]
  )
  const canAddReplacementPlayers = user?.is_admin === true

  const error =
    addMemberMutation.error ||
    createTeamMutation.error ||
    renameTeamMutation.error ||
    deleteTeamMutation.error ||
    changeCaptainMutation.error ||
    removeMemberMutation.error ||
    moveMemberMutation.error ||
    assignSignupMutation.error ||
    deleteSignupMutation.error

  const isBusy =
    addMemberMutation.isPending ||
    createTeamMutation.isPending ||
    renameTeamMutation.isPending ||
    deleteTeamMutation.isPending ||
    changeCaptainMutation.isPending ||
    removeMemberMutation.isPending ||
    moveMemberMutation.isPending ||
    assignSignupMutation.isPending ||
    deleteSignupMutation.isPending

  const handleCreateTeam = () => {
    if (!newTeamName.trim()) return
    createTeamMutation.mutate(
      { tournamentId, name: newTeamName.trim() },
      {
        onSuccess: () => {
          setFeedback('Team wurde erstellt.')
          setNewTeamName('')
        },
      }
    )
  }

  const handleRenameTeam = (teamId: number) => {
    const name = renameValues[teamId]?.trim()
    if (!name) return
    renameTeamMutation.mutate(
      { tournamentId, teamId, name },
      { onSuccess: () => setFeedback('Teamname gespeichert.') }
    )
  }

  const handleDeleteTeam = (teamId: number, teamName: string) => {
    if (!window.confirm(`Team "${teamName}" wirklich löschen?`)) return
    deleteTeamMutation.mutate(
      { tournamentId, teamId },
      { onSuccess: () => setFeedback('Team wurde gelöscht.') }
    )
  }

  const handleChangeCaptain = (teamId: number, discordId: string) => {
    changeCaptainMutation.mutate(
      { tournamentId, teamId, discordId },
      { onSuccess: () => setFeedback('Captain wurde aktualisiert.') }
    )
  }

  const handleRemoveMember = (teamId: number, member: TeamMember) => {
    if (!window.confirm(`${playerLabel(member)} aus dem Team entfernen?`)) return
    removeMemberMutation.mutate(
      { tournamentId, teamId, discordId: member.discord_id },
      { onSuccess: () => setFeedback('Spieler wurde aus dem Team entfernt.') }
    )
  }

  const handleMoveMember = (fromTeamId: number, member: TeamMember) => {
    const key = `${fromTeamId}:${member.discord_id}`
    const targetTeamId = Number(moveTargets[key])
    if (!targetTeamId) return
    moveMemberMutation.mutate(
      {
        tournamentId,
        targetTeamId,
        data: { from_team_id: fromTeamId, discord_id: member.discord_id },
      },
      {
        onSuccess: () => {
          setFeedback('Spieler wurde verschoben.')
          setMoveTargets((current) => ({ ...current, [key]: '' }))
        },
      }
    )
  }

  const handleAssignSignup = (signupId: number) => {
    const teamId = Number(signupTargets[signupId])
    if (!teamId) return
    assignSignupMutation.mutate(
      { tournamentId, teamId, signupId },
      {
        onSuccess: () => {
          setFeedback('Solo-Anmeldung wurde einem Team zugewiesen.')
          setSignupTargets((current) => ({ ...current, [signupId]: '' }))
        },
      }
    )
  }

  const handleDeleteSignup = (signupId: number, label: string) => {
    if (!window.confirm(`Solo-Anmeldung von ${label} löschen?`)) return
    deleteSignupMutation.mutate(
      { tournamentId, signupId },
      { onSuccess: () => setFeedback('Solo-Anmeldung wurde gelöscht.') }
    )
  }

  const handleAddReplacement = () => {
    if (!replacementTeam || !replacementDiscordId.trim() || !replacementDiscordName.trim()) return
    addMemberMutation.mutate(
      {
        teamId: replacementTeam.id,
        discordId: replacementDiscordId.trim(),
        discordName: replacementDiscordName.trim(),
      },
      {
        onSuccess: () => {
          setFeedback('Ersatzspieler wurde hinzugefügt.')
          setReplacementTeam(null)
          setReplacementDiscordId('')
          setReplacementDiscordName('')
        },
      }
    )
  }

  return (
    <div className="space-y-6">
      <Card className="p-6 space-y-4">
        <div className="flex items-center gap-2">
          <Users size={18} className="text-primary" />
          <h2 className="text-lg font-semibold text-foreground">Teams & Teilnehmer</h2>
        </div>
        <p className="text-sm text-muted">
          Teams erstellen, Spieler verschieben, Captains setzen und offene Solo-Anmeldungen zuweisen.
        </p>
        {canAddReplacementPlayers && ['group_phase', 'bracket'].includes(tournamentStatus) && (
          <p className="text-sm text-muted">
            Ersatzspieler werden direkt über die jeweilige Teamkarte hinzugefügt.
          </p>
        )}

        <div className="flex flex-col gap-2 sm:flex-row">
          <input
            type="text"
            value={newTeamName}
            onChange={(event) => setNewTeamName(event.target.value)}
            placeholder="Neues Team anlegen..."
            className="flex-1 rounded-lg border border-border bg-background px-3 py-2 text-foreground focus:outline-none focus:ring-2 focus:ring-primary/50"
          />
          <Button variant="primary" size="sm" disabled={isBusy || !newTeamName.trim()} onClick={handleCreateTeam}>
            <Plus size={14} />
            {createTeamMutation.isPending ? 'Erstellt...' : 'Team erstellen'}
          </Button>
        </div>

        {feedback && (
          <div className="rounded-lg border border-green-500/20 bg-green-500/10 p-3 text-sm text-green-400">
            {feedback}
          </div>
        )}

        {error && (
          <div className="flex items-center gap-2 rounded-lg border border-red-500/20 bg-red-500/10 p-3 text-sm text-red-400">
            <AlertCircle size={16} />
            <span>{error instanceof Error ? error.message : 'Ein Fehler ist aufgetreten'}</span>
          </div>
        )}
      </Card>

      <Card className="p-6 space-y-4">
        <div className="flex items-center justify-between gap-3">
          <div>
            <h3 className="text-base font-semibold text-foreground">Offene Solo-Anmeldungen</h3>
            <p className="text-sm text-muted">{pendingSignups.length} wartend auf Team-Zuweisung</p>
          </div>
        </div>

        {pendingSignups.length === 0 ? (
          <p className="text-sm text-muted">Keine offenen Solo-Anmeldungen vorhanden.</p>
        ) : (
          <div className="space-y-3">
            {pendingSignups.map((signup) => (
              <div key={signup.id} className="rounded-xl border border-border bg-background/60 p-4">
                <div className="flex flex-col gap-3 lg:flex-row lg:items-center lg:justify-between">
                  <div className="space-y-1">
                    <div className="font-medium text-foreground">{signupName(signup)}</div>
                    <div className="text-xs text-muted">{signup.discord_id}</div>
                    <div className="text-xs text-muted">
                      {signup.rank ? `${signup.rank} · Score ${signup.rank_score}` : `Score ${signup.rank_score}`}
                    </div>
                  </div>

                  <div className="flex flex-col gap-2 sm:flex-row">
                    <select
                      value={signupTargets[signup.id] ?? ''}
                      onChange={(event) => setSignupTargets((current) => ({ ...current, [signup.id]: event.target.value }))}
                      className="rounded-lg border border-border bg-card px-3 py-2 text-sm text-foreground focus:outline-none focus:ring-2 focus:ring-primary/50"
                    >
                      <option value="">Team wählen...</option>
                      {teams
                        .filter((team) => team.members.length < teamSize)
                        .map((team) => (
                          <option key={team.id} value={team.id}>
                            {team.name} ({team.members.length}/{teamSize})
                          </option>
                        ))}
                    </select>
                    <Button
                      variant="secondary"
                      size="sm"
                      disabled={isBusy || !signupTargets[signup.id]}
                      onClick={() => handleAssignSignup(signup.id)}
                    >
                      <UserPlus size={14} />
                      Zuweisen
                    </Button>
                    <Button
                      variant="ghost"
                      size="sm"
                      disabled={isBusy}
                      onClick={() => handleDeleteSignup(signup.id, signupName(signup))}
                    >
                      <Trash2 size={14} />
                      Entfernen
                    </Button>
                  </div>
                </div>
              </div>
            ))}
          </div>
        )}
      </Card>

      <div className="grid gap-4 xl:grid-cols-2">
        {teams.map((team) => (
          <Card key={team.id} className="p-5 space-y-4">
            <div className="flex flex-col gap-3">
              <div className="flex flex-col gap-2 sm:flex-row">
                <input
                  type="text"
                  value={renameValues[team.id] ?? team.name}
                  onChange={(event) =>
                    setRenameValues((current) => ({ ...current, [team.id]: event.target.value }))
                  }
                  className="flex-1 rounded-lg border border-border bg-background px-3 py-2 text-foreground focus:outline-none focus:ring-2 focus:ring-primary/50"
                />
                <Button
                  variant="secondary"
                  size="sm"
                  disabled={isBusy || !(renameValues[team.id] ?? '').trim()}
                  onClick={() => handleRenameTeam(team.id)}
                >
                  Speichern
                </Button>
                <Button
                  variant="danger"
                  size="sm"
                  disabled={isBusy}
                  onClick={() => handleDeleteTeam(team.id, team.name)}
                >
                  <Trash2 size={14} />
                  Löschen
                </Button>
              </div>

              <div className="flex flex-wrap items-center gap-3 text-sm text-muted">
                <span>{team.members.length}/{teamSize} Mitglieder</span>
                <span>Captain: {captainLabel(team)}</span>
              </div>

              {canAddReplacementPlayers && ['group_phase', 'bracket'].includes(tournamentStatus) && (
                <div>
                  <Button
                    variant="secondary"
                    size="sm"
                    disabled={isBusy || team.members.length >= teamSize}
                    onClick={() => setReplacementTeam(team)}
                  >
                    <UserPlus size={14} />
                    Spieler ersetzen
                  </Button>
                </div>
              )}

              {team.members.length > 0 && (
                <div className="flex flex-col gap-2 sm:flex-row sm:items-center">
                  <div className="flex items-center gap-2 text-sm text-muted">
                    <Crown size={14} />
                    <span>Captain wechseln</span>
                  </div>
                  <select
                    value={team.captain_discord_id}
                    onChange={(event) => handleChangeCaptain(team.id, event.target.value)}
                    className="rounded-lg border border-border bg-card px-3 py-2 text-sm text-foreground focus:outline-none focus:ring-2 focus:ring-primary/50"
                  >
                    {team.members.map((member) => (
                      <option key={member.discord_id} value={member.discord_id}>
                        {playerLabel(member)}
                      </option>
                    ))}
                  </select>
                </div>
              )}
            </div>

            {team.members.length === 0 ? (
              <p className="text-sm text-muted">Dieses Team ist aktuell leer.</p>
            ) : (
              <div className="space-y-3">
                {team.members.map((member) => {
                  const moveKey = `${team.id}:${member.discord_id}`
                  const availableTargets = teams.filter(
                    (candidate) => candidate.id !== team.id && candidate.members.length < teamSize
                  )

                  return (
                    <div key={member.discord_id} className="rounded-lg border border-border bg-background/60 p-3">
                      <div className="flex flex-col gap-3">
                        <div className="flex items-start justify-between gap-3">
                          <div>
                            <div className="font-medium text-foreground">{playerLabel(member)}</div>
                            <div className="text-xs text-muted">{member.discord_id}</div>
                            <div className="text-xs text-muted">
                              {member.role === 'captain' ? 'Captain' : 'Mitglied'}
                              {member.rank ? ` · ${member.rank}` : ''}
                              {member.rank_score ? ` · Score ${member.rank_score}` : ''}
                            </div>
                          </div>

                          <Button
                            variant="ghost"
                            size="sm"
                            disabled={isBusy}
                            onClick={() => handleRemoveMember(team.id, member)}
                          >
                            <UserMinus size={14} />
                            Entfernen
                          </Button>
                        </div>

                        {availableTargets.length > 0 && (
                          <div className="flex flex-col gap-2 sm:flex-row">
                            <select
                              value={moveTargets[moveKey] ?? ''}
                              onChange={(event) =>
                                setMoveTargets((current) => ({ ...current, [moveKey]: event.target.value }))
                              }
                              className="flex-1 rounded-lg border border-border bg-card px-3 py-2 text-sm text-foreground focus:outline-none focus:ring-2 focus:ring-primary/50"
                            >
                              <option value="">In anderes Team verschieben...</option>
                              {availableTargets.map((candidate) => (
                                <option key={candidate.id} value={candidate.id}>
                                  {candidate.name} ({candidate.members.length}/{teamSize})
                                </option>
                              ))}
                            </select>
                            <Button
                              variant="secondary"
                              size="sm"
                              disabled={isBusy || !moveTargets[moveKey]}
                              onClick={() => handleMoveMember(team.id, member)}
                            >
                              <ArrowRightLeft size={14} />
                              Verschieben
                            </Button>
                          </div>
                        )}
                      </div>
                    </div>
                  )
                })}
              </div>
            )}
          </Card>
        ))}
      </div>

      {replacementTeam && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 px-4">
          <div className="w-full max-w-md rounded-xl border border-border bg-card p-6 shadow-xl">
            <h3 className="text-lg font-semibold text-foreground">Ersatzspieler für {replacementTeam.name}</h3>
            <p className="mt-1 text-sm text-muted">
              Fügt einen Discord-User direkt zum Team hinzu. Falls das Team voll ist, entferne zuerst ein Mitglied.
            </p>

            <div className="mt-4 space-y-3">
              <input
                type="text"
                value={replacementDiscordId}
                onChange={(event) => setReplacementDiscordId(event.target.value)}
                placeholder="Discord ID"
                className="w-full rounded-lg border border-border bg-background px-3 py-2 text-foreground focus:outline-none focus:ring-2 focus:ring-primary/50"
              />
              <input
                type="text"
                value={replacementDiscordName}
                onChange={(event) => setReplacementDiscordName(event.target.value)}
                placeholder="Discord Name"
                className="w-full rounded-lg border border-border bg-background px-3 py-2 text-foreground focus:outline-none focus:ring-2 focus:ring-primary/50"
              />
            </div>

            <div className="mt-5 flex justify-end gap-2">
              <Button
                variant="ghost"
                size="sm"
                disabled={isBusy}
                onClick={() => {
                  setReplacementTeam(null)
                  setReplacementDiscordId('')
                  setReplacementDiscordName('')
                }}
              >
                Abbrechen
              </Button>
              <Button
                variant="primary"
                size="sm"
                disabled={isBusy || !replacementDiscordId.trim() || !replacementDiscordName.trim()}
                onClick={handleAddReplacement}
              >
                {addMemberMutation.isPending ? 'Fügt hinzu...' : 'Ersatzspieler hinzufügen'}
              </Button>
            </div>
          </div>
        </div>
      )}
    </div>
  )
}
