export function remainingSeconds(
  deadlineAt: string | null | undefined,
  nowMs: number,
): number | null {
  if (!deadlineAt) return null
  return Math.max(0, Math.ceil((new Date(deadlineAt).getTime() - nowMs) / 1000))
}

export function heroImageUrl(imageUrl: string): string | null {
  return imageUrl.trim() || null
}

export function heroCardImageUrl(cardImageUrl: string | undefined | null, portraitUrl: string): string | null {
  return cardImageUrl?.trim() || heroImageUrl(portraitUrl)
}

export function countdownText(restSekunden: number | null): string {
  if (restSekunden === null) return ''
  const minuten = Math.floor(restSekunden / 60)
  const rest = restSekunden % 60
  return `${minuten}:${String(rest).padStart(2, '0')}`
}

export interface PhasenKopf {
  label: string
  anteil: string
  action: 'ban' | 'pick'
  team: 1 | 2
}

export function phasenKopf(
  sequenz: { index: number; team: 1 | 2; action: 'ban' | 'pick' }[],
  aktuellerIndex: number,
): PhasenKopf | null {
  const schritt = sequenz[aktuellerIndex]
  if (!schritt) return null
  const gesamt = sequenz.length
  return {
    label: schritt.action === 'ban' ? 'BAN-PHASE' : 'PICK-PHASE',
    anteil: `${schritt.index + 1}/${gesamt}`,
    action: schritt.action,
    team: schritt.team,
  }
}

export function claimSpeicherSchluessel(code: string): string {
  return `draft-claim:${code}`
}

export function leseClaimToken(code: string): string | null {
  return window.sessionStorage.getItem(claimSpeicherSchluessel(code))
}

export function schreibeClaimToken(code: string, token: string) {
  window.sessionStorage.setItem(claimSpeicherSchluessel(code), token)
}

export function loescheClaimToken(code: string) {
  window.sessionStorage.removeItem(claimSpeicherSchluessel(code))
}

export function raumUrl(code: string): string {
  return `${window.location.origin}${import.meta.env.BASE_URL}draft/${code}`
}
