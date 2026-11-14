import { useContext } from 'react'
import { DuplicateNamesContext } from '@/components/admin/duplicateNamesContext'
import { isGuestParticipant, participantNameKey } from '@/lib/participantName'

interface ParticipantMarksProps {
  discordId: string
  name: string | null
}

export default function ParticipantMarks({ discordId, name }: ParticipantMarksProps) {
  const duplicates = useContext(DuplicateNamesContext)
  const guest = isGuestParticipant(discordId)
  const duplicate = duplicates.has(participantNameKey(name))
  if (!guest && !duplicate) return null
  return (
    <span className="ml-1.5 inline-flex flex-wrap gap-1 align-middle">
      {guest && (
        <span className="rounded-full border border-border px-1.5 py-px text-[10px] font-medium text-muted">Gast</span>
      )}
      {duplicate && (
        <span
          className="rounded-full bg-amber-500/15 px-1.5 py-px text-[10px] font-medium text-amber-300"
          title="Mehrere Teilnehmer haben diesen Namen. Prüfe, ob dieselbe Person doppelt angemeldet ist."
        >
          Name doppelt
        </span>
      )}
    </span>
  )
}
