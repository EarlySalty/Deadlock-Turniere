export const GOLD = '#c8a86b'
export const BLAU = '#3b82f6'
export const ROT = '#ef4444'
export const BEREIT_GRUEN = '#10b981'
export const TINTE = '#0b0b0b'

export function teamFarbe(team: 1 | 2): string {
  return team === 1 ? GOLD : BLAU
}
