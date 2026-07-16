export function remainingSeconds(
  deadlineAt: string | null | undefined,
  nowMs: number,
): number | null {
  if (!deadlineAt) return null
  return Math.max(0, Math.ceil((new Date(deadlineAt).getTime() - nowMs) / 1000))
}

export function selectCaptainToken(
  code: string | undefined,
  urlToken: string | null,
  storedToken: string | null,
): string | null {
  if (!code) return null
  return urlToken || storedToken
}

export function heroImageUrl(imageUrl: string): string | null {
  return imageUrl.trim() || null
}
