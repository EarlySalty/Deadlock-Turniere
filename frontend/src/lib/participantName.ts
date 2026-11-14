export const PARTICIPANT_NAME_MAX_LENGTH = 32

export function isGuestParticipant(discordId: string): boolean {
  return discordId.startsWith('-')
}

export function participantNameKey(name: string | null): string {
  return (name ?? '').trim().toLocaleLowerCase('de-DE')
}

export function duplicateNameKeys(names: Array<string | null>): Set<string> {
  const seen = new Set<string>()
  const duplicates = new Set<string>()
  for (const name of names) {
    const key = participantNameKey(name)
    if (!key) continue
    if (seen.has(key)) duplicates.add(key)
    seen.add(key)
  }
  return duplicates
}
