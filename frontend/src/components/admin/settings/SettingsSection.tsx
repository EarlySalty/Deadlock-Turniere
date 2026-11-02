import type { ReactNode } from 'react'
import { ChevronDown } from 'lucide-react'

interface SettingsSectionProps {
  title: string
  description: string
  children: ReactNode
}

interface CollapsibleSectionProps extends SettingsSectionProps {
  summary: string
  open: boolean
  onToggle: (open: boolean) => void
}

export function SettingsSection({ title, description, children }: SettingsSectionProps) {
  return (
    <section className="space-y-4 rounded-xl border border-border bg-background/30 p-4 sm:p-5">
      <header>
        <h3 className="text-base font-semibold text-foreground">{title}</h3>
        <p className="mt-0.5 text-sm text-muted">{description}</p>
      </header>
      {children}
    </section>
  )
}

export function CollapsibleSection({
  title,
  description,
  summary,
  open,
  onToggle,
  children,
}: CollapsibleSectionProps) {
  return (
    <details
      open={open}
      onToggle={(event) => onToggle(event.currentTarget.open)}
      className="group rounded-xl border border-border bg-background/30"
    >
      <summary className="flex cursor-pointer list-none items-start justify-between gap-3 p-4 sm:p-5 [&::-webkit-details-marker]:hidden">
        <div>
          <h3 className="text-base font-semibold text-foreground">{title}</h3>
          <p className="mt-0.5 text-sm text-muted">{description}</p>
          <p className="mt-2 text-xs text-foreground/80">{summary}</p>
        </div>
        <ChevronDown size={18} className="mt-1 shrink-0 text-muted transition-transform group-open:rotate-180" />
      </summary>
      <div className="space-y-4 border-t border-border p-4 sm:p-5">{children}</div>
    </details>
  )
}

export function SubHeading({ children }: { children: ReactNode }) {
  return (
    <h4 className="text-xs font-semibold uppercase tracking-wider text-muted">{children}</h4>
  )
}
