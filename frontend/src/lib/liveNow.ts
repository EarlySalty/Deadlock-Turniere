import { bracketStageLabel } from './bracketStage'
import type { BracketMatch, MatchStatus, TournamentDetailPublic } from '../types/tournament'

export interface NowMatch {
  key: string
  stage: string
  team1Id: number | null
  team2Id: number | null
  status: MatchStatus
  scheduledAt: string | null
  order: number
}

export type NowState =
  | { kind: 'live'; matches: NowMatch[] }
  | { kind: 'next'; matches: NowMatch[] }
  | { kind: 'start'; checkinStart: string | null; start: string | null }
  | { kind: 'none' }

const LIVE_STATUSES: MatchStatus[] = ['checkin', 'lobby_created', 'in_progress']
const RUNNING_TOURNAMENT = ['group_phase', 'bracket']
const BEFORE_START = ['registration', 'checkin']
const MAX_MATCHES = 4

type NowSource = Pick<TournamentDetailPublic, 'status' | 'bracket_matches' | 'groups' | 'mini_groups' | 'checkin_start'>

function bracketStage(match: BracketMatch, source: NowSource): string {
  const group = source.mini_groups.find((entry) => entry.id === match.mini_group_id)
  if (group) return `Runde ${group.round} · Slot ${group.position + 1}`
  return bracketStageLabel(match, source.bracket_matches)
}

function allMatches(source: NowSource): NowMatch[] {
  const bracket = source.bracket_matches.map((match) => ({
    key: `bracket-${match.id}`,
    stage: bracketStage(match, source),
    team1Id: match.team1_id,
    team2Id: match.team2_id,
    status: match.status,
    scheduledAt: match.scheduled_at,
    order: 100_000 + match.round * 1000 + match.position,
  }))
  const groups = source.groups.flatMap((group) => group.matches.map((match) => ({
    key: `group-${match.id}`,
    stage: match.round ? `${group.name} · Runde ${match.round}` : group.name,
    team1Id: match.team1_id,
    team2Id: match.team2_id,
    status: match.status,
    scheduledAt: match.scheduled_at,
    order: (match.round ?? 0) * 1000 + match.id,
  })))
  return [...groups, ...bracket]
}

function byScheduleThenOrder(left: NowMatch, right: NowMatch): number {
  const leftTime = left.scheduledAt ? Date.parse(left.scheduledAt) : Number.MAX_SAFE_INTEGER
  const rightTime = right.scheduledAt ? Date.parse(right.scheduledAt) : Number.MAX_SAFE_INTEGER
  return leftTime - rightTime || left.order - right.order
}

export function nowState(source: NowSource, start: string | null): NowState {
  if (BEFORE_START.includes(source.status)) {
    return start || source.checkin_start ? { kind: 'start', checkinStart: source.checkin_start, start } : { kind: 'none' }
  }
  if (!RUNNING_TOURNAMENT.includes(source.status)) return { kind: 'none' }
  const matches = allMatches(source)
  const live = matches.filter((match) => LIVE_STATUSES.includes(match.status)).sort(byScheduleThenOrder)
  if (live.length > 0) return { kind: 'live', matches: live.slice(0, MAX_MATCHES) }
  const ready = matches
    .filter((match) => match.status === 'pending' && match.team1Id !== null && match.team2Id !== null)
    .sort(byScheduleThenOrder)
  if (ready.length === 0) return { kind: 'none' }
  return { kind: 'next', matches: ready.slice(0, MAX_MATCHES) }
}
