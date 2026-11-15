export interface SubmittedAction<TError> {
  error: TError | null
  submittedAt: number
}

export function latestActionError<TError>(actions: SubmittedAction<TError>[]): TError | null {
  let latest: SubmittedAction<TError> | null = null
  for (const action of actions) {
    if (action.submittedAt > 0 && (latest === null || action.submittedAt > latest.submittedAt)) latest = action
  }
  return latest?.error ?? null
}
