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

function knownTeamKeys(snapshot: ServerSnapshot, edits: BuilderEdits): Set<string> {
  return new Set([...snapshot.teams.map((team) => team.key), ...edits.newTeams.map((team) => team.key)])
}

export function finalLocations(snapshot: ServerSnapshot, edits: BuilderEdits): Map<string, string | null> {
  const locations = baseLocations(snapshot)
  const teamKeys = knownTeamKeys(snapshot, edits)
  for (const [id, target] of Object.entries(edits.placements)) {
    if (!locations.has(id)) continue
    if (target === null || teamKeys.has(target)) locations.set(id, target)
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

export function nameKey(name: string): string {
  return name.trim().toLowerCase().replace(/ß/g, 'ss')
}

interface NameOp {
  teamKey: string
  name: string
  create: boolean
}

function nameOps(snapshot: ServerSnapshot, edits: BuilderEdits): NameOp[] {
  const creates = edits.newTeams.map((team) => ({
    teamKey: team.key,
    name: edits.names[team.key] ?? team.name,
    create: true,
  }))
  const renames = snapshot.teams
    .filter((team) => edits.names[team.key] !== undefined && edits.names[team.key] !== team.name)
    .map((team) => ({ teamKey: team.key, name: edits.names[team.key], create: false }))
  return [...creates, ...renames]
}

function nameStep(op: NameOp): SaveStep {
  return { kind: op.create ? 'create' : 'rename', teamKey: op.teamKey, name: op.name }
}

function isNameFree(occupied: Map<string, string>, op: NameOp): boolean {
  const owner = occupied.get(nameKey(op.name))
  return owner === undefined || owner === op.teamKey
}

function takeName(occupied: Map<string, string>, teamKey: string, name: string) {
  for (const [key, owner] of occupied) if (owner === teamKey) occupied.delete(key)
  occupied.set(nameKey(name), teamKey)
}

function cycleBreaker(occupied: Map<string, string>, pending: NameOp[]): NameOp | undefined {
  const owners = pending.map((op) => occupied.get(nameKey(op.name)))
  return pending.find((op) => !op.create && owners.includes(op.teamKey))
}

function temporaryName(occupied: Map<string, string>, pending: NameOp[]): string {
  const reserved = new Set([...occupied.keys(), ...pending.map((op) => nameKey(op.name))])
  let index = 1
  while (reserved.has(nameKey(`Umbenennung ${index}`))) index += 1
  return `Umbenennung ${index}`
}

function nameSteps(snapshot: ServerSnapshot, edits: BuilderEdits): SaveStep[] {
  const occupied = new Map(snapshot.teams.map((team) => [nameKey(team.name), team.key]))
  const steps: SaveStep[] = []
  let pending = nameOps(snapshot, edits)
  while (pending.length > 0) {
    const ready = pending.find((op) => isNameFree(occupied, op))
    if (ready) {
      steps.push(nameStep(ready))
      takeName(occupied, ready.teamKey, ready.name)
      pending = pending.filter((op) => op !== ready)
      continue
    }
    const blocker = cycleBreaker(occupied, pending)
    if (!blocker) return [...steps, ...pending.map(nameStep)]
    const name = temporaryName(occupied, pending)
    steps.push({ kind: 'rename', teamKey: blocker.teamKey, name })
    takeName(occupied, blocker.teamKey, name)
  }
  return steps
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
    ...nameSteps(snapshot, edits),
    ...removals,
    ...orderMoves(pendingMoves(snapshot, edits), counts, teamSize),
  ]
}

interface SimulatedTeam {
  members: string[]
  captain: string | null
}

function leaveTeam(team: SimulatedTeam | undefined, discordId: string) {
  if (!team) return
  team.members = team.members.filter((id) => id !== discordId)
  if (team.captain === discordId) team.captain = team.members[0] ?? null
}

function joinTeam(team: SimulatedTeam | undefined, discordId: string) {
  if (!team) return
  if (team.members.length === 0 || team.captain === null) team.captain = discordId
  team.members.push(discordId)
}

function simulateStep(teams: Map<string, SimulatedTeam>, step: SaveStep) {
  if (step.kind === 'remove') leaveTeam(teams.get(step.teamKey), step.discordId)
  if (step.kind === 'move') leaveTeam(teams.get(step.fromKey), step.discordId)
  if (step.kind === 'move' || step.kind === 'assign') joinTeam(teams.get(step.teamKey), step.discordId)
}

export function captainsAfterSteps(
  snapshot: ServerSnapshot,
  edits: BuilderEdits,
  steps: SaveStep[],
): Map<string, string | null> {
  const teams = new Map<string, SimulatedTeam>(
    snapshot.teams.map((team) => [team.key, { members: [...team.members], captain: team.captainId }]),
  )
  for (const team of edits.newTeams) teams.set(team.key, { members: [], captain: null })
  for (const step of steps) simulateStep(teams, step)
  return new Map([...teams].map(([key, team]) => [key, team.captain]))
}

export interface CaptainWish {
  teamKey: string
  discordId: string
}

export function displayedCaptains(view: BuilderView): CaptainWish[] {
  return view.teams.flatMap((team) => (team.captainId ? [{ teamKey: team.key, discordId: team.captainId }] : []))
}

export function captainChanges(
  view: BuilderView,
  snapshot: ServerSnapshot,
  edits: BuilderEdits,
  steps: SaveStep[],
): CaptainWish[] {
  const after = captainsAfterSteps(snapshot, edits, steps)
  return displayedCaptains(view).filter((wish) => after.get(wish.teamKey) !== wish.discordId)
}

export interface TemporaryRename {
  teamKey: string
  temporary: string
  original: string
  intended: string
}

export function temporaryRenames(snapshot: ServerSnapshot, edits: BuilderEdits, steps: SaveStep[]): TemporaryRename[] {
  return snapshot.teams.flatMap((team) => {
    const intended = edits.names[team.key]
    const step = steps.find((s) => s.kind === 'rename' && s.teamKey === team.key && s.name !== intended)
    if (!step || step.kind !== 'rename' || intended === undefined) return []
    return [{ teamKey: team.key, temporary: step.name, original: team.name, intended }]
  })
}

function restoredName(snapshot: ServerSnapshot, rename: TemporaryRename): string {
  const taken = snapshot.teams.some(
    (team) => team.key !== rename.teamKey && nameKey(team.name) === nameKey(rename.original),
  )
  return taken ? rename.intended : rename.original
}

export function restoreTemporaryNames(snapshot: ServerSnapshot, renames: TemporaryRename[]): BuilderEdits {
  const names: Record<string, string> = {}
  for (const rename of renames) {
    const team = snapshot.teams.find((t) => t.key === rename.teamKey)
    if (team && team.name === rename.temporary) names[team.key] = restoredName(snapshot, rename)
  }
  return { ...EMPTY_EDITS, names }
}

export function pinCaptains(edits: BuilderEdits, view: BuilderView): BuilderEdits {
  const captains = { ...edits.captains }
  for (const wish of displayedCaptains(view)) captains[wish.teamKey] = wish.discordId
  return { ...edits, captains }
}

export function rebaseEdits(edits: BuilderEdits, teamIds: Map<string, number>): BuilderEdits {
  const created = new Map(
    edits.newTeams.flatMap((team) => {
      const id = teamIds.get(team.key)
      return id === undefined ? [] : [[team.key, serverTeamKey(id)] as const]
    }),
  )
  const rekey = (key: string) => created.get(key) ?? key
  const names = Object.fromEntries(Object.entries(edits.names).map(([key, name]) => [rekey(key), name]))
  for (const team of edits.newTeams) {
    if (created.has(team.key)) names[rekey(team.key)] = edits.names[team.key] ?? team.name
  }
  return {
    newTeams: edits.newTeams.filter((team) => !created.has(team.key)),
    names,
    placements: Object.fromEntries(
      Object.entries(edits.placements).map(([id, key]) => [id, key === null ? null : rekey(key)]),
    ),
    captains: Object.fromEntries(Object.entries(edits.captains).map(([key, id]) => [rekey(key), id])),
  }
}
