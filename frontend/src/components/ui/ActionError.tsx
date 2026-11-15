import { AlertCircle } from 'lucide-react'
import { latestActionError, type SubmittedAction } from '@/lib/latestError'

interface ActionErrorProps {
  actions: SubmittedAction<Error>[]
  className?: string
}

export default function ActionError({ actions, className = '' }: ActionErrorProps) {
  const error = latestActionError(actions)
  if (!error) return null
  return (
    <div
      role="alert"
      className={`flex items-start gap-2 rounded-lg border border-red-500/20 bg-red-500/10 p-3 text-sm text-red-400 ${className}`}
    >
      <AlertCircle size={16} className="mt-0.5 shrink-0" />
      <span className="min-w-0 break-words">{error.message}</span>
    </div>
  )
}
