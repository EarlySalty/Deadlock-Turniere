import type { BracketMatch } from '../types/tournament'
import { isDoubleElimination } from './bracketStage.ts'

export interface BracketPlacement {
  position: number
  teamId: number
  label: string
}

const SHARED_THIRD_LABEL = '3./4. Platz'

type PlacementMatch = Pick<BracketMatch, 'round' | 'bracket_type' | 'team1_id' | 'team2_id' | 'winner_id' | 'status'>

function isDecided(match: PlacementMatch): boolean {
  return (match.status === 'completed' || match.status === 'forfeit') && match.winner_id !== null
}

function loserOf(match: PlacementMatch): number | null {
  return match.team1_id === match.winner_id ? match.team2_id : match.team1_id
}

function keepBest(entries: BracketPlacement[]): BracketPlacement[] {
  const seen = new Map<number, BracketPlacement>()
  for (const entry of entries) {
    const existing = seen.get(entry.teamId)
    if (!existing || entry.position < existing.position) {
      seen.set(entry.teamId, entry)
    }
  }
  return Array.from(seen.values()).sort((a, b) => a.position - b.position)
}

function singleEliminationPlacements(matches: PlacementMatch[]): BracketPlacement[] {
  const completed = matches.filter((m) => m.bracket_type === 'winners' && isDecided(m))
  if (completed.length === 0) return []

  const maxRound = Math.max(...completed.map((m) => m.round))
  const entries: BracketPlacement[] = []

  for (const match of completed) {
    const distance = maxRound - match.round
    const winnerId = match.winner_id!
    const loserId = loserOf(match)

    if (distance === 0) {
      entries.push({ position: 1, teamId: winnerId, label: '1. Platz' })
      if (loserId !== null) {
        entries.push({ position: 2, teamId: loserId, label: '2. Platz' })
      }
    } else {
      const startPos = Math.pow(2, distance) + 1
      const label = distance === 1 ? SHARED_THIRD_LABEL : distance === 2 ? '5.–8. Platz' : '9.+ Platz'
      if (loserId !== null) {
        entries.push({ position: startPos, teamId: loserId, label })
      }
    }
  }

  return keepBest(entries)
}

function rangeLabel(start: number, size: number): string {
  return size <= 1 ? `${start}. Platz` : `${start}.–${start + size - 1}. Platz`
}

export function placementLabel(placement: number | null | undefined, shared: boolean | undefined, fallback: string): string {
  if (placement === null || placement === undefined || !Number.isInteger(placement) || placement < 1) return fallback
  if (shared === true && placement === 3) return SHARED_THIRD_LABEL
  return rangeLabel(placement, 1)
}

function doubleEliminationPlacements(matches: PlacementMatch[]): BracketPlacement[] {
  const entries: BracketPlacement[] = []
  const grandFinal = matches
    .filter((m) => m.bracket_type === 'grand_final' && m.status !== 'cancelled')
    .sort((a, b) => b.round - a.round)[0]
  if (grandFinal && isDecided(grandFinal)) {
    entries.push({ position: 1, teamId: grandFinal.winner_id!, label: '1. Platz' })
    const runnerUp = loserOf(grandFinal)
    if (runnerUp !== null) {
      entries.push({ position: 2, teamId: runnerUp, label: '2. Platz' })
    }
  }

  const losers = matches.filter((m) => m.bracket_type === 'losers')
  const rounds = Array.from(new Set(losers.map((m) => m.round))).sort((a, b) => b - a)
  let start = 3
  for (const round of rounds) {
    const roundMatches = losers.filter((m) => m.round === round)
    const label = rangeLabel(start, roundMatches.length)
    for (const match of roundMatches.filter(isDecided)) {
      const loserId = loserOf(match)
      if (loserId !== null) {
        entries.push({ position: start, teamId: loserId, label })
      }
    }
    start += roundMatches.length
  }

  return keepBest(entries)
}

export function bracketPlacements(matches: PlacementMatch[]): BracketPlacement[] {
  return isDoubleElimination(matches) ? doubleEliminationPlacements(matches) : singleEliminationPlacements(matches)
}
