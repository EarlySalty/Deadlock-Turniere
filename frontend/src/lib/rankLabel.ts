const RANK_NAMES = [
  'Initiate',
  'Seeker',
  'Alchemist',
  'Arcanist',
  'Ritualist',
  'Emissary',
  'Archon',
  'Oracle',
  'Phantom',
  'Ascendant',
  'Eternus',
]

export function rankLabel(rankScore: number, rank: string | null): string {
  const tier = Math.floor((rankScore - 1) / 6)
  const name = RANK_NAMES[tier - 1]
  if (!Number.isInteger(rankScore) || !name) return rank?.trim() || 'Ohne Rang'
  return `${name} ${rankScore - tier * 6}`
}
