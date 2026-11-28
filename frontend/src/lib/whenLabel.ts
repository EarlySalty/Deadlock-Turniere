function sameDay(left: Date, right: Date): boolean {
  return left.getFullYear() === right.getFullYear()
    && left.getMonth() === right.getMonth()
    && left.getDate() === right.getDate()
}

function dayLabel(date: Date, now: Date): string {
  if (sameDay(date, now)) return 'heute'
  const tomorrow = new Date(now)
  tomorrow.setDate(now.getDate() + 1)
  if (sameDay(date, tomorrow)) return 'morgen'
  return date.toLocaleDateString('de-DE', { weekday: 'short', day: '2-digit', month: '2-digit' })
}

export function whenLabel(value: string | null | undefined, now: Date = new Date()): string | null {
  if (!value) return null
  const date = new Date(value)
  if (Number.isNaN(date.getTime())) return null
  const time = date.toLocaleTimeString('de-DE', { hour: '2-digit', minute: '2-digit' })
  return `${dayLabel(date, now)}, ${time} Uhr`
}
