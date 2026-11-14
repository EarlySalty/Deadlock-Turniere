import { EMPTY_EDITS, type BuilderEdits, type TemporaryRename } from '@/lib/teamBuilder'

export interface TeamBuilderDraft {
  edits: BuilderEdits
  renames: TemporaryRename[]
}

export interface PendingDraftChanges {
  here: number
  otherTabs: number
}

interface DraftEntry {
  draft: TeamBuilderDraft
  pending: number
}

interface SharedPending {
  pending: number
  savedAt: number
}

const EMPTY_DRAFT: TeamBuilderDraft = { edits: EMPTY_EDITS, renames: [] }
const PREFIX = 'turniere.teamBuilderDraft.'
const MAX_AGE_MS = 6 * 60 * 60 * 1000
const tabId = `${Date.now().toString(36)}${Math.random().toString(36).slice(2, 8)}`
const drafts = new Map<number, DraftEntry>()

function sharedKey(tournamentId: number): string {
  return `${PREFIX}${tournamentId}.${tabId}`
}

function withStorage(action: (storage: Storage) => void) {
  try {
    action(window.localStorage)
  } catch {
    return
  }
}

function isEmpty(draft: TeamBuilderDraft): boolean {
  const { edits } = draft
  return (
    draft.renames.length === 0 &&
    edits.newTeams.length === 0 &&
    Object.keys(edits.names).length === 0 &&
    Object.keys(edits.placements).length === 0 &&
    Object.keys(edits.captains).length === 0
  )
}

function freshPending(raw: string | null): number {
  if (!raw) return 0
  try {
    const value = JSON.parse(raw) as Partial<SharedPending>
    if (typeof value.pending !== 'number' || typeof value.savedAt !== 'number') return 0
    return Date.now() - value.savedAt < MAX_AGE_MS ? Math.max(0, value.pending) : 0
  } catch {
    return 0
  }
}

function forgetSharedEntries() {
  withStorage((storage) => {
    for (const tournamentId of drafts.keys()) storage.removeItem(sharedKey(tournamentId))
  })
}

if (typeof window !== 'undefined') window.addEventListener('pagehide', forgetSharedEntries)

export function readDraft(tournamentId: number): TeamBuilderDraft {
  return drafts.get(tournamentId)?.draft ?? EMPTY_DRAFT
}

export function writeDraft(tournamentId: number, draft: TeamBuilderDraft, changeCount: number) {
  const key = sharedKey(tournamentId)
  if (isEmpty(draft) && changeCount === 0) {
    drafts.delete(tournamentId)
    withStorage((storage) => storage.removeItem(key))
    return
  }
  drafts.set(tournamentId, { draft, pending: changeCount })
  const shared: SharedPending = { pending: changeCount, savedAt: Date.now() }
  withStorage((storage) => storage.setItem(key, JSON.stringify(shared)))
}

export function pendingDraftChanges(tournamentId: number): PendingDraftChanges {
  const here = drafts.get(tournamentId)?.pending ?? 0
  let otherTabs = 0
  withStorage((storage) => {
    const prefix = `${PREFIX}${tournamentId}.`
    const own = sharedKey(tournamentId)
    for (let index = 0; index < storage.length; index += 1) {
      const key = storage.key(index)
      if (key && key.startsWith(prefix) && key !== own) otherTabs += freshPending(storage.getItem(key))
    }
  })
  return { here, otherTabs }
}
