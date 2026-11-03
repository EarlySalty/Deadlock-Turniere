export type ReminderUnit = 'minutes' | 'hours' | 'days' | 'weeks'

export const DEFAULT_REMINDER_OFFSETS = [1440, 120, 15]
export const DEFAULT_START_REMINDER_OFFSETS = [1440, 60]

const MINUTES_PER_UNIT: Record<ReminderUnit, number> = {
  minutes: 1,
  hours: 60,
  days: 1440,
  weeks: 10080,
}

export const REMINDER_UNITS: { value: ReminderUnit; label: string }[] = [
  { value: 'minutes', label: 'Minuten' },
  { value: 'hours', label: 'Stunden' },
  { value: 'days', label: 'Tage' },
  { value: 'weeks', label: 'Wochen' },
]

export const REMINDER_PRESETS: { label: string; minutes: number }[] = [
  { label: '1 Tag', minutes: 1440 },
  { label: '2 Std', minutes: 120 },
  { label: '1 Std', minutes: 60 },
  { label: '15 Min', minutes: 15 },
]

export function toMinutes(amount: number, unit: ReminderUnit): number | null {
  if (!Number.isInteger(amount) || amount <= 0) return null
  return amount * MINUTES_PER_UNIT[unit]
}

export function sortOffsets(offsets: number[]): number[] {
  return [...new Set(offsets.filter((entry) => Number.isInteger(entry) && entry >= 0))].sort(
    (left, right) => right - left
  )
}

export function addOffset(offsets: number[], minutes: number): number[] {
  return sortOffsets([...offsets, minutes])
}

export function removeOffset(offsets: number[], minutes: number): number[] {
  return offsets.filter((entry) => entry !== minutes)
}

export function offsetsOrDefault(offsets: number[], defaults: number[]): number[] {
  return offsets.length > 0 ? sortOffsets(offsets) : defaults
}

function pluralize(count: number, singular: string, plural: string): string {
  return `${count} ${count === 1 ? singular : plural}`
}

export function formatOffset(minutes: number): string {
  if (minutes === 0) return 'Zum Start'
  if (minutes % 10080 === 0) return `${pluralize(minutes / 10080, 'Woche', 'Wochen')} vorher`
  if (minutes % 1440 === 0) return `${pluralize(minutes / 1440, 'Tag', 'Tage')} vorher`
  if (minutes % 60 === 0) return `${minutes / 60} Std vorher`
  return `${minutes} Min vorher`
}
