import { useContext } from 'react'
import { ParticipantMarksContext } from '@/components/admin/participantMarksContext'

interface ParticipantMarksProps {
  discordId: string
}

export default function ParticipantMarks({ discordId }: ParticipantMarksProps) {
  const marks = useContext(ParticipantMarksContext)
  const guest = marks.guestIds.has(discordId)
  const duplicate = marks.duplicateNameIds.has(discordId)
  if (!guest && !duplicate) return null
  return (
    <span className="ml-1.5 inline-flex shrink-0 flex-wrap gap-1 align-middle">
      {guest && (
        <span className="whitespace-nowrap rounded-full border border-border px-1.5 py-px text-[10px] font-medium text-muted">Gast</span>
      )}
      {duplicate && (
        <span
          className="whitespace-nowrap rounded-full bg-amber-500/15 px-1.5 py-px text-[10px] font-medium text-amber-300"
          title="Mehrere Teilnehmer haben diesen Namen. Prüfe, ob dieselbe Person doppelt angemeldet ist."
        >
          Name doppelt
        </span>
      )}
    </span>
  )
}
