import { createContext } from 'react'

export interface ParticipantMarkIds {
  guestIds: ReadonlySet<string>
  duplicateNameIds: ReadonlySet<string>
}

export const ParticipantMarksContext = createContext<ParticipantMarkIds>({
  guestIds: new Set(),
  duplicateNameIds: new Set(),
})
