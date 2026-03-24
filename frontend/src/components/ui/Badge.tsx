import type { TournamentStatus } from '@/types/tournament'

interface BadgeProps {
  status: TournamentStatus
  className?: string
}

const statusConfig: Record<TournamentStatus, { label: string; color: string }> = {
  draft: { label: 'Entwurf', color: 'bg-muted/20 text-muted' },
  registration: { label: 'Anmeldung', color: 'bg-success/20 text-success' },
  group_phase: { label: 'Gruppenphase', color: 'bg-blue-500/20 text-blue-400' },
  bracket: { label: 'Bracket', color: 'bg-primary/20 text-primary' },
  completed: { label: 'Abgeschlossen', color: 'bg-muted/20 text-muted' },
  archived: { label: 'Archiviert', color: 'bg-muted/10 text-muted' },
}

export default function Badge({ status, className = '' }: BadgeProps) {
  const config = statusConfig[status]
  return (
    <span className={`inline-flex items-center rounded-full px-2.5 py-0.5 text-xs font-medium ${config.color} ${className}`}>
      {config.label}
    </span>
  )
}
