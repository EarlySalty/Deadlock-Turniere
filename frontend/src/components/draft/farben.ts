export const GOLD = '#c8a86b'
export const BLAU = '#7caec2'
export const ROT = '#c85e4d'
export const BEREIT_GRUEN = '#70b69c'
export const TINTE = '#0b0b0b'

export function teamFarbe(team: 1 | 2): string {
  return team === 1 ? GOLD : BLAU
}
