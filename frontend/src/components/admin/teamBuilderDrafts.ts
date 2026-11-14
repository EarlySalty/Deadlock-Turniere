import { EMPTY_EDITS, type BuilderEdits, type TemporaryRename } from '@/lib/teamBuilder'

export interface TeamBuilderDraft {
  edits: BuilderEdits
  renames: TemporaryRename[]
}

const EMPTY_DRAFT: TeamBuilderDraft = { edits: EMPTY_EDITS, renames: [] }

const drafts = new Map<number, TeamBuilderDraft>()
const pendingChanges = new Map<number, number>()

export function readDraft(tournamentId: number): TeamBuilderDraft {
  return drafts.get(tournamentId) ?? EMPTY_DRAFT
}

export function writeDraft(tournamentId: number, draft: TeamBuilderDraft, changeCount: number) {
  const empty = draft.edits === EMPTY_EDITS && draft.renames.length === 0
  if (empty) drafts.delete(tournamentId)
  else drafts.set(tournamentId, draft)
  if (changeCount > 0) pendingChanges.set(tournamentId, changeCount)
  else pendingChanges.delete(tournamentId)
}

export function pendingDraftChanges(tournamentId: number): number {
  return pendingChanges.get(tournamentId) ?? 0
}
