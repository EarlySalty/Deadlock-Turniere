import { pluralWord } from './countLabel.ts'

interface Seat {
  is_bench?: boolean
}

export interface Lineup {
  starters: number
  bench: number
  starterFree: number
  benchFree: number
}

export function starterCount(members: readonly Seat[]): number {
  return members.filter((member) => !member.is_bench).length
}

export function lineupOf(members: readonly Seat[], teamSize: number, benchSlots: number): Lineup {
  const starters = starterCount(members)
  const bench = members.length - starters
  return {
    starters,
    bench,
    starterFree: Math.max(teamSize - starters, 0),
    benchFree: Math.max((benchSlots ?? 0) - bench, 0),
  }
}

export function isTeamComplete(members: readonly Seat[], teamSize: number): boolean {
  return starterCount(members) >= teamSize
}

export function teamFormatLabel(teamSize: number, benchSlots: number): string {
  const base = `${teamSize} gegen ${teamSize}`
  return benchSlots > 0 ? `${base} · bis zu ${benchSlots} Ersatz` : base
}

export function benchFreeLabel(benchFree: number): string {
  return `${benchFree} ${pluralWord(benchFree, 'Ersatzplatz', 'Ersatzplätze')} frei`
}
