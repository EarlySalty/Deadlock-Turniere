import { useMemo, useState } from 'react'
import Card from '@/components/ui/Card'
import Button from '@/components/ui/Button'
import {
  useAssignSignupToAdminTeam,
  useChangeAdminCaptain,
  useCreateAdminTeam,
  useDeleteAdminSignup,
  useDeleteAdminTeam,
  useMoveAdminTeamMember,
  useRemoveAdminTeamMember,
  useRenameAdminTeam,
} from '@/hooks/useTournament'
import type { Team, TeamMember, TournamentSignup } from '@/types/tournament'
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
  teamSize: number
  teams: Team[]
  signups: TournamentSignup[]
}

function playerLabel(member: Pick<TeamMember, 'discord_id' | 'discord_name'>): string {
  return member.discord_name ?? member.discord_id
}

function signupLabel(signup: TournamentSignup): string {
  return signup.discord_id
}

export default function ParticipantManager({
  tournamentId,
  teamSize,
  teams,
  signups,
}: ParticipantManagerProps) {
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
  const [feedback, setFeedback] = useState('')

  const pendingSignups = useMemo(
    () => signups.filter((signup) => signup.team_id === null),
    [signups]
  )

  const error =
    createTeamMutation.error ||
    renameTeamMutation.error ||
    deleteTeamMutation.error ||
    changeCaptainMutation.error ||
    removeMemberMutation.error ||
    moveMemberMutation.error ||
    assignSignupMutation.error ||
    deleteSignupMutation.error

  const isBusy =
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
                    <div className="font-medium text-foreground">{signupLabel(signup)}</div>
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
                      onClick={() => handleDeleteSignup(signup.id, signupLabel(signup))}
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
                <span>Captain: {team.captain_discord_id || 'noch keiner gesetzt'}</span>
              </div>

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
    </div>
  )
}
