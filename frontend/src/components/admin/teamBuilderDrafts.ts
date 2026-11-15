import { useEffect, useState } from 'react'
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
const HEARTBEAT_MS = 20_000
const BACKGROUND_TIMER_FLOOR_MS = 60_000
const STALE_AFTER_MS = 3 * BACKGROUND_TIMER_FLOOR_MS
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

function freshPending(raw: string | null): number | null {
  if (!raw) return null
  try {
    const value = JSON.parse(raw) as Partial<SharedPending>
    if (typeof value.pending !== 'number' || typeof value.savedAt !== 'number') return null
    return Date.now() - value.savedAt < STALE_AFTER_MS ? Math.max(0, value.pending) : null
  } catch {
    return null
  }
}

function publish(tournamentId: number, pending: number) {
  const shared: SharedPending = { pending, savedAt: Date.now() }
  withStorage((storage) => storage.setItem(sharedKey(tournamentId), JSON.stringify(shared)))
}

function publishAll() {
  for (const [tournamentId, entry] of drafts) publish(tournamentId, entry.pending)
}

function forgetSharedEntries() {
  withStorage((storage) => {
    for (const tournamentId of drafts.keys()) storage.removeItem(sharedKey(tournamentId))
  })
}

if (typeof window !== 'undefined') {
  window.addEventListener('pagehide', forgetSharedEntries)
  window.addEventListener('pageshow', publishAll)
  window.setInterval(publishAll, HEARTBEAT_MS)
}

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
  publish(tournamentId, changeCount)
}

function pendingDraftChanges(tournamentId: number): PendingDraftChanges {
  const here = drafts.get(tournamentId)?.pending ?? 0
  let otherTabs = 0
  withStorage((storage) => {
    const prefix = `${PREFIX}${tournamentId}.`
    const own = sharedKey(tournamentId)
    const stale: string[] = []
    for (let index = 0; index < storage.length; index += 1) {
      const key = storage.key(index)
      if (!key || !key.startsWith(prefix) || key === own) continue
      const pending = freshPending(storage.getItem(key))
      if (pending === null) stale.unshift(key)
      else otherTabs += pending
    }
    for (const key of stale) storage.removeItem(key)
  })
  return { here, otherTabs }
}

export function usePendingDraftChanges(tournamentId: number, active: boolean): PendingDraftChanges {
  const [, setTick] = useState(0)
  useEffect(() => {
    if (!active) return
    const refresh = () => setTick((tick) => tick + 1)
    window.addEventListener('storage', refresh)
    const timer = window.setInterval(refresh, HEARTBEAT_MS)
    return () => {
      window.removeEventListener('storage', refresh)
      window.clearInterval(timer)
    }
  }, [active])
  return active ? pendingDraftChanges(tournamentId) : { here: 0, otherTabs: 0 }
}
