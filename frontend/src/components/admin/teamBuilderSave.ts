import {
  assignSignupToAdminTeam,
  changeAdminCaptain,
  createAdminTeam,
  fetchAdminTournament,
  moveAdminTeamMember,
  removeAdminTeamMember,
  renameAdminTeam,
} from '@/api/client'
import type { CaptainWish, SaveStep } from '@/lib/teamBuilder'

export interface SaveContext {
  tournamentId: number
  teamIds: Map<string, number>
  teamNames: Map<string, string>
  playerNames: Map<string, string>
}

export class SaveStepError extends Error {}

function teamIdOf(ctx: SaveContext, key: string): number {
  const id = ctx.teamIds.get(key)
  if (id === undefined) throw new SaveStepError(`Team „${ctx.teamNames.get(key) ?? key}“ wurde nicht angelegt.`)
  return id
}

async function resolveSignupId(ctx: SaveContext, discordId: string): Promise<number> {
  const detail = await fetchAdminTournament(ctx.tournamentId)
  const signup = detail.signups.find((s) => s.discord_id === discordId && s.team_id === null)
  if (!signup) throw new SaveStepError('Die Anmeldung des Spielers wurde nicht gefunden.')
  return signup.id
}

export function describeStep(ctx: SaveContext, step: SaveStep): string {
  const team = ctx.teamNames.get(step.teamKey) ?? 'Team'
  if (step.kind === 'create') return `Team „${step.name}“ anlegen`
  if (step.kind === 'rename') return `Team in „${step.name}“ umbenennen`
  const player = ctx.playerNames.get(step.discordId) ?? 'Spieler'
  if (step.kind === 'remove') return `${player} aus „${team}“ entfernen`
  return `${player} in „${team}“ eintragen`
}

async function runStep(ctx: SaveContext, step: SaveStep): Promise<void> {
  const { tournamentId } = ctx
  if (step.kind === 'create') {
    const team = await createAdminTeam(tournamentId, step.name)
    ctx.teamIds.set(step.teamKey, team.id)
    return
  }
  if (step.kind === 'rename') {
    await renameAdminTeam(tournamentId, teamIdOf(ctx, step.teamKey), step.name)
    return
  }
  if (step.kind === 'remove') {
    await removeAdminTeamMember(tournamentId, teamIdOf(ctx, step.teamKey), step.discordId)
    return
  }
  if (step.kind === 'move') {
    await moveAdminTeamMember(tournamentId, teamIdOf(ctx, step.teamKey), {
      from_team_id: teamIdOf(ctx, step.fromKey),
      discord_id: step.discordId,
    })
    return
  }
  const signupId = step.signupId ?? (await resolveSignupId(ctx, step.discordId))
  await assignSignupToAdminTeam(tournamentId, teamIdOf(ctx, step.teamKey), signupId)
}

async function applyCaptains(ctx: SaveContext, wishes: CaptainWish[]): Promise<void> {
  if (wishes.length === 0) return
  const detail = await fetchAdminTournament(ctx.tournamentId)
  for (const wish of wishes) {
    const team = detail.teams.find((t) => t.id === ctx.teamIds.get(wish.teamKey))
    if (!team || team.captain_discord_id === wish.discordId) continue
    if (!team.members.some((m) => m.discord_id === wish.discordId)) continue
    await changeAdminCaptain(ctx.tournamentId, team.id, wish.discordId)
  }
}

function errorText(error: unknown): string {
  return error instanceof Error ? error.message : 'Unbekannter Fehler'
}

export async function saveTeamBuilder(
  ctx: SaveContext,
  steps: SaveStep[],
  captains: CaptainWish[],
  onProgress: (done: number, total: number) => void,
): Promise<void> {
  const total = steps.length + (captains.length > 0 ? 1 : 0)
  for (const [index, step] of steps.entries()) {
    onProgress(index, total)
    try {
      await runStep(ctx, step)
    } catch (error) {
      throw new SaveStepError(`${describeStep(ctx, step)} ist fehlgeschlagen: ${errorText(error)}`)
    }
  }
  onProgress(steps.length, total)
  try {
    await applyCaptains(ctx, captains)
  } catch (error) {
    throw new SaveStepError(`Captain setzen ist fehlgeschlagen: ${errorText(error)}`)
  }
  onProgress(total, total)
}
