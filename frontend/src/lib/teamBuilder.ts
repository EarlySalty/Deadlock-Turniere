import type { Team, TournamentSignup } from '@/types/tournament'

export interface BuilderPlayer {
  discordId: string
  name: string
  rank: string | null
  rankScore: number
  signupId: number | null
  signedUpAt: string | null
}

export interface BuilderEdits {
  newTeams: { key: string; name: string }[]
  names: Record<string, string>
  placements: Record<string, string | null>
  captains: Record<string, string>
}

export interface BuilderTeamView {
  key: string
  serverId: number | null
  name: string
  members: BuilderPlayer[]
  captainId: string | null
}

export interface BuilderView {
  teams: BuilderTeamView[]
  pool: BuilderPlayer[]
}

export interface ServerSnapshot {
  teams: { key: string; id: number; name: string; captainId: string | null; members: string[] }[]
  pool: { discordId: string; signupId: number }[]
}

export type SaveStep =
  | { kind: 'create'; teamKey: string; name: string }
  | { kind: 'rename'; teamKey: string; name: string }
  | { kind: 'remove'; teamKey: string; discordId: string }
  | { kind: 'move'; fromKey: string; teamKey: string; discordId: string }
  | { kind: 'assign'; teamKey: string; discordId: string; signupId: number | null }

export const EMPTY_EDITS: BuilderEdits = { newTeams: [], names: {}, placements: {}, captains: {} }

export function serverTeamKey(teamId: number): string {
  return `s${teamId}`
}

export function playerName(name: string | null): string {
  return name?.trim() || 'Unbekannt'
}

function playersFromServer(teams: Team[], signups: TournamentSignup[]): Map<string, BuilderPlayer> {
  const players = new Map<string, BuilderPlayer>()
  for (const team of teams) {
    for (const member of team.members) {
      players.set(member.discord_id, {
        discordId: member.discord_id,
        name: playerName(member.discord_name),
        rank: member.rank,
        rankScore: member.rank_score,
        signupId: null,
        signedUpAt: null,
      })
    }
  }
  for (const signup of signups) {
    players.set(signup.discord_id, {
      discordId: signup.discord_id,
      name: playerName(signup.discord_name),
      rank: signup.rank,
      rankScore: signup.rank_score,
      signupId: signup.team_id === null ? signup.id : null,
      signedUpAt: signup.signed_up_at,
    })
  }
  return players
}

export function snapshotOf(teams: Team[], signups: TournamentSignup[]): ServerSnapshot {
  const members = new Set(teams.flatMap((team) => team.members.map((m) => m.discord_id)))
  return {
    teams: teams.map((team) => ({
      key: serverTeamKey(team.id),
      id: team.id,
      name: team.name,
      captainId: team.members.some((m) => m.discord_id === team.captain_discord_id)
        ? team.captain_discord_id
        : null,
      members: team.members.map((m) => m.discord_id),
    })),
    pool: signups
      .filter((signup) => signup.team_id === null && !members.has(signup.discord_id))
      .map((signup) => ({ discordId: signup.discord_id, signupId: signup.id })),
  }
}

function baseLocations(snapshot: ServerSnapshot): Map<string, string | null> {
  const locations = new Map<string, string | null>()
  for (const team of snapshot.teams) {
    for (const id of team.members) locations.set(id, team.key)
  }
  for (const entry of snapshot.pool) locations.set(entry.discordId, null)
  return locations
}

export function finalLocations(snapshot: ServerSnapshot, edits: BuilderEdits): Map<string, string | null> {
  const locations = baseLocations(snapshot)
  for (const [id, target] of Object.entries(edits.placements)) {
    if (locations.has(id)) locations.set(id, target)
  }
  return locations
}

function orderedIds(snapshot: ServerSnapshot, edits: BuilderEdits): string[] {
  const base = [...baseLocations(snapshot).keys()]
  const moved = Object.keys(edits.placements).filter((id) => base.includes(id))
  return [...base.filter((id) => !moved.includes(id)), ...moved]
}

function resolveCaptain(key: string, memberIds: string[], snapshot: ServerSnapshot, edits: BuilderEdits): string | null {
  const wanted = edits.captains[key]
  if (wanted && memberIds.includes(wanted)) return wanted
  const current = snapshot.teams.find((team) => team.key === key)?.captainId ?? null
  if (current && memberIds.includes(current)) return current
  return memberIds[0] ?? null
}

export function buildView(teams: Team[], signups: TournamentSignup[], edits: BuilderEdits): BuilderView {
  const players = playersFromServer(teams, signups)
  const snapshot = snapshotOf(teams, signups)
  const locations = finalLocations(snapshot, edits)
  const ids = orderedIds(snapshot, edits)
  const membersOf = (key: string | null) =>
    ids.filter((id) => locations.get(id) === key).flatMap((id) => players.get(id) ?? [])
  const teamKeys = [
    ...snapshot.teams.map((team) => ({ key: team.key, serverId: team.id, name: team.name })),
    ...edits.newTeams.map((team) => ({ key: team.key, serverId: null, name: team.name })),
  ]
  return {
    teams: teamKeys.map((team) => {
      const members = membersOf(team.key)
      const memberIds = members.map((m) => m.discordId)
      return {
        key: team.key,
        serverId: team.serverId,
        name: edits.names[team.key] ?? team.name,
        members,
        captainId: resolveCaptain(team.key, memberIds, snapshot, edits),
      }
    }),
    pool: membersOf(null),
  }
}

function structuralSteps(snapshot: ServerSnapshot, edits: BuilderEdits): SaveStep[] {
  const creates: SaveStep[] = edits.newTeams.map((team) => ({
    kind: 'create',
    teamKey: team.key,
    name: edits.names[team.key] ?? team.name,
  }))
  const renames: SaveStep[] = snapshot.teams
    .filter((team) => edits.names[team.key] !== undefined && edits.names[team.key] !== team.name)
    .map((team) => ({ kind: 'rename', teamKey: team.key, name: edits.names[team.key] }))
  return [...creates, ...renames]
}

interface PendingMove {
  discordId: string
  from: string | null
  to: string
  signupId: number | null
}

function pendingMoves(snapshot: ServerSnapshot, edits: BuilderEdits): PendingMove[] {
  const base = baseLocations(snapshot)
  const target = finalLocations(snapshot, edits)
  const signups = new Map(snapshot.pool.map((entry) => [entry.discordId, entry.signupId]))
  const moves: PendingMove[] = []
  for (const [id, to] of target) {
    const from = base.get(id) ?? null
    if (to === null || to === from) continue
    moves.push({ discordId: id, from, to, signupId: signups.get(id) ?? null })
  }
  return moves
}

function removalSteps(snapshot: ServerSnapshot, edits: BuilderEdits): SaveStep[] {
  const base = baseLocations(snapshot)
  const target = finalLocations(snapshot, edits)
  const steps: SaveStep[] = []
  for (const [id, to] of target) {
    const from = base.get(id) ?? null
    if (to === null && from !== null) steps.push({ kind: 'remove', teamKey: from, discordId: id })
  }
  return steps
}

function initialCounts(snapshot: ServerSnapshot, edits: BuilderEdits): Map<string, number> {
  const counts = new Map<string, number>()
  for (const team of snapshot.teams) counts.set(team.key, team.members.length)
  for (const team of edits.newTeams) counts.set(team.key, 0)
  return counts
}

function stepForMove(move: PendingMove): SaveStep {
  if (move.from === null) {
    return { kind: 'assign', teamKey: move.to, discordId: move.discordId, signupId: move.signupId }
  }
  return { kind: 'move', fromKey: move.from, teamKey: move.to, discordId: move.discordId }
}

function shift(counts: Map<string, number>, key: string | null, delta: number) {
  if (key !== null) counts.set(key, (counts.get(key) ?? 0) + delta)
}

function orderMoves(moves: PendingMove[], counts: Map<string, number>, teamSize: number): SaveStep[] {
  const steps: SaveStep[] = []
  let pending = [...moves]
  while (pending.length > 0) {
    const ready = pending.find((move) => (counts.get(move.to) ?? 0) < teamSize)
    if (ready) {
      steps.push(stepForMove(ready))
      shift(counts, ready.from, -1)
      shift(counts, ready.to, 1)
      pending = pending.filter((move) => move !== ready)
      continue
    }
    const [blocked, ...rest] = pending
    if (blocked.from === null) return [...steps, ...pending.map(stepForMove)]
    steps.push({ kind: 'remove', teamKey: blocked.from, discordId: blocked.discordId })
    shift(counts, blocked.from, -1)
    pending = [...rest, { ...blocked, from: null, signupId: null }]
  }
  return steps
}

export function planSave(snapshot: ServerSnapshot, edits: BuilderEdits, teamSize: number): SaveStep[] {
  const counts = initialCounts(snapshot, edits)
  const removals = removalSteps(snapshot, edits)
  for (const step of removals) shift(counts, step.teamKey, -1)
  return [
    ...structuralSteps(snapshot, edits),
    ...removals,
    ...orderMoves(pendingMoves(snapshot, edits), counts, teamSize),
  ]
}

export function captainChanges(
  view: BuilderView,
  snapshot: ServerSnapshot,
  edits: BuilderEdits,
): { teamKey: string; discordId: string }[] {
  const current = new Map(snapshot.teams.map((team) => [team.key, team.captainId]))
  return view.teams
    .filter((team) => {
      const wanted = edits.captains[team.key]
      return wanted !== undefined && team.captainId === wanted && current.get(team.key) !== wanted
    })
    .map((team) => ({ teamKey: team.key, discordId: edits.captains[team.key] }))
}

export function countChanges(
  view: BuilderView,
  snapshot: ServerSnapshot,
  edits: BuilderEdits,
  teamSize: number,
): number {
  return planSave(snapshot, edits, teamSize).length + captainChanges(view, snapshot, edits).length
}
