import type { TournamentStatus } from '@/types/tournament'

interface BadgeProps {
  status: TournamentStatus
  className?: string
}

const statusConfig: Record<TournamentStatus, { label: string; style: string }> = {
  draft: { label: 'Entwurf', style: 'border-white/10 text-muted bg-white/5' },
  registration: { label: 'Anmeldung', style: 'border-success/30 text-success bg-success/10' },
  checkin: { label: 'Check-in', style: 'border-amber-500/30 text-amber-300 bg-amber-500/10' },
  group_phase: { label: 'Gruppenphase', style: 'border-accent/30 text-accent bg-accent/10' },
  bracket: { label: 'Bracket', style: 'border-primary/30 text-primary bg-primary/10' },
  completed: { label: 'Abgeschlossen', style: 'border-white/10 text-muted bg-white/5' },
  archived: { label: 'Archiviert', style: 'border-white/10 text-muted bg-white/5 opacity-50' },
}

export default function Badge({ status, className = '' }: BadgeProps) {
  const config = statusConfig[status]
  return (
    <span className={`inline-flex items-center rounded-md border px-2 py-0.5 text-[10px] font-bold uppercase tracking-widest ${config.style} ${className}`}>
      {config.label}
    </span>
  )
}
