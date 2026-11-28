import type { BracketMatch } from '../types/tournament'

type LinkedMatch = Pick<BracketMatch, 'id' | 'round' | 'position' | 'bracket_type' | 'mini_group_id'> &
  Partial<Pick<BracketMatch, 'source_match1_id' | 'source_match2_id' | 'loser_to_match_id'>>

function inMiniGroup(match: LinkedMatch): boolean {
  return match.mini_group_id !== null && match.mini_group_id !== undefined
}

function nextGrandFinal(match: LinkedMatch, matches: LinkedMatch[]): LinkedMatch[] {
  const later = matches
    .filter((m) => m.bracket_type === 'grand_final' && m.id !== match.id && m.round > match.round)
    .sort((a, b) => a.round - b.round)
  return later.slice(0, 1)
}

function legacyNext(match: LinkedMatch, matches: LinkedMatch[]): LinkedMatch[] {
  if (match.bracket_type !== 'winners' || inMiniGroup(match)) return []
  const next = matches.find(
    (m) =>
      m.bracket_type === 'winners' &&
      !inMiniGroup(m) &&
      m.round === match.round + 1 &&
      m.position === Math.trunc(match.position / 2),
  )
  return next ? [next] : []
}

function followersOf(match: LinkedMatch, matches: LinkedMatch[]): LinkedMatch[] {
  if (match.bracket_type === 'grand_final') return nextGrandFinal(match, matches)
  const direct = matches.filter((m) => m.source_match1_id === match.id || m.source_match2_id === match.id)
  const winnerPath = direct.length > 0 ? direct : legacyNext(match, matches)
  const loserPath = matches.filter((m) => m.id === match.loser_to_match_id)
  return [...winnerPath, ...loserPath]
}

export function reachesMiniGroup(match: LinkedMatch, matches: LinkedMatch[], seen: Set<number> = new Set()): boolean {
  if (inMiniGroup(match)) return true
  if (seen.has(match.id)) return false
  seen.add(match.id)
  return followersOf(match, matches).some((follower) => reachesMiniGroup(follower, matches, seen))
}
