import { EMPTY_EDITS, type BuilderEdits, type TemporaryRename } from '@/lib/teamBuilder'

export interface TeamBuilderDraft {
  edits: BuilderEdits
  renames: TemporaryRename[]
}

const EMPTY_DRAFT: TeamBuilderDraft = { edits: EMPTY_EDITS, renames: [] }

const drafts = new Map<number, TeamBuilderDraft>()

export function readDraft(tournamentId: number): TeamBuilderDraft {
  return drafts.get(tournamentId) ?? EMPTY_DRAFT
}

export function writeDraft(tournamentId: number, draft: TeamBuilderDraft) {
  const empty = draft.edits === EMPTY_EDITS && draft.renames.length === 0
  if (empty) drafts.delete(tournamentId)
  else drafts.set(tournamentId, draft)
}
