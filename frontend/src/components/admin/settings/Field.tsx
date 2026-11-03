import type { ReactNode } from 'react'

interface FieldProps {
  id: string
  label: string
  hint?: ReactNode
  className?: string
  children: ReactNode
}

export default function Field({ id, label, hint, className = '', children }: FieldProps) {
  return (
    <div className={className}>
      <label htmlFor={id} className="mb-1.5 block text-sm font-medium text-foreground">
        {label}
      </label>
      {children}
      {hint && <p className="mt-1 text-xs text-muted">{hint}</p>}
    </div>
  )
}
