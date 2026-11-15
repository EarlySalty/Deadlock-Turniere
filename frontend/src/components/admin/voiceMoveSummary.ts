import type { VoiceMoveResult } from '@/types/tournament'
import { pluralize } from '@/lib/countLabel'

export interface VoiceMoveSummary {
  kind: 'success' | 'error'
  text: string
}

function guestHint(guests: number): string {
  return guests > 0 ? `. ${pluralize(guests, 'Gast', 'Gäste')} ohne Discord-Login bitte selbst verschieben` : ''
}

export function summarizeVoiceMove(results: VoiceMoveResult[]): VoiceMoveSummary {
  const moved = results.reduce((sum, result) => sum + result.moved.length, 0)
  const failed = results.flatMap((result) => result.failed)
  const guests = results.reduce((sum, result) => sum + result.skipped_guests.length, 0)
  const movedText = `${pluralize(moved, 'Spieler', 'Spieler')} verschoben`
  if (failed.length > 0) {
    const reasons = [...new Set(failed.map((entry) => entry.error))].join('; ')
    return { kind: 'error', text: `${movedText}, ${failed.length} fehlgeschlagen: ${reasons}${guestHint(guests)}` }
  }
  if (moved === 0) {
    return {
      kind: 'error',
      text: guests > 0 ? `Niemand verschoben${guestHint(guests)}` : 'Niemand verschoben: keine Spieler mit Discord-Login gefunden',
    }
  }
  return { kind: guests > 0 ? 'error' : 'success', text: movedText + guestHint(guests) }
}
