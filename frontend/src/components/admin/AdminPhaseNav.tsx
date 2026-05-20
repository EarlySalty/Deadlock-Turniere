import {
  Check,
  CheckCheck,
  GitBranch,
  Headphones,
  LayoutGrid,
  Settings,
  Users,
  type LucideIcon,
} from 'lucide-react'
import type { TournamentStatus } from '@/types/tournament'

export type AdminPhase =
  | 'setup'
  | 'participants'
  | 'checkin'
  | 'group_phase'
  | 'bracket'
  | 'voice'

type PhaseState = 'done' | 'current' | 'upcoming'

interface PhaseConfig {
  id: AdminPhase
  label: string
  icon: LucideIcon
  /** Status-Rang, zu dem diese Phase gehört (für die Fortschrittsanzeige). */
  statusRank: number
  isAvailable: (status: TournamentStatus, hasGroups: boolean, hasBracket: boolean) => boolean
}

const STATUS_RANK: Record<TournamentStatus, number> = {
  draft: 0,
  registration: 1,
  checkin: 2,
  group_phase: 3,
  bracket: 4,
  completed: 5,
  archived: 5,
}

const PHASE_CONFIG: PhaseConfig[] = [
  {
    id: 'setup',
    label: 'Setup',
    icon: Settings,
    statusRank: 0,
    isAvailable: () => true,
  },
  {
    id: 'participants',
    label: 'Teilnehmer',
    icon: Users,
    statusRank: 1,
    isAvailable: (status) =>
      ['draft', 'registration', 'checkin', 'group_phase', 'bracket'].includes(status),
  },
  {
    id: 'checkin',
    label: 'Check-in',
    icon: CheckCheck,
    statusRank: 2,
    isAvailable: (status) => status === 'checkin',
  },
  {
    id: 'group_phase',
    label: 'Gruppenphase',
    icon: LayoutGrid,
    statusRank: 3,
    isAvailable: (_status, hasGroups) => hasGroups,
  },
  {
    id: 'bracket',
    label: 'Matches',
    icon: GitBranch,
    statusRank: 4,
    isAvailable: (_status, _hasGroups, hasBracket) => hasBracket,
  },
  {
    id: 'voice',
    label: 'Voice & Caster',
    icon: Headphones,
    statusRank: 4,
    isAvailable: (_status, _hasGroups, hasBracket) => hasBracket,
  },
]

interface AdminPhaseNavProps {
  status: TournamentStatus
  hasGroups: boolean
  hasBracket: boolean
  activePhase: AdminPhase
  onChange: (phase: AdminPhase) => void
}

export function defaultPhaseFor(
  status: TournamentStatus,
  hasGroups: boolean,
  hasBracket: boolean,
): AdminPhase {
  if (status === 'draft' || status === 'registration') return 'participants'
  if (status === 'checkin') return 'checkin'
  if (status === 'group_phase') return hasGroups ? 'group_phase' : 'participants'
  if (status === 'bracket') return hasBracket ? 'bracket' : 'participants'
  if (status === 'completed' || status === 'archived') {
    if (hasBracket) return 'bracket'
    if (hasGroups) return 'group_phase'
  }
  return 'setup'
}

function phaseState(phase: PhaseConfig, status: TournamentStatus): PhaseState {
  const currentRank = STATUS_RANK[status] ?? 0
  if (phase.statusRank < currentRank) return 'done'
  if (phase.statusRank > currentRank) return 'upcoming'
  return 'current'
}

export default function AdminPhaseNav({
  status,
  hasGroups,
  hasBracket,
  activePhase,
  onChange,
}: AdminPhaseNavProps) {
  const visible = PHASE_CONFIG.filter((phase) => phase.isAvailable(status, hasGroups, hasBracket))

  return (
    <nav
      role="tablist"
      aria-label="Turnier-Lebenszyklus"
      className="flex flex-col gap-1 rounded-xl border border-border bg-card p-2"
    >
      <div className="px-2 pb-1 pt-1 text-[10px] uppercase tracking-wider text-muted">
        Phase
      </div>
      {visible.map((phase) => {
        const Icon = phase.icon
        const isActive = activePhase === phase.id
        const state = phaseState(phase, status)
        return (
          <button
            key={phase.id}
            type="button"
            role="tab"
            aria-selected={isActive}
            onClick={() => onChange(phase.id)}
            className={`group flex items-center gap-2.5 rounded-lg px-2.5 py-2 text-left transition-colors ${
              isActive
                ? 'bg-primary/15 text-primary'
                : state === 'upcoming'
                  ? 'text-muted hover:bg-card-hover'
                  : 'text-foreground/85 hover:bg-card-hover hover:text-foreground'
            }`}
          >
            <span
              className={`flex h-5 w-5 shrink-0 items-center justify-center rounded-full border text-[10px] ${
                state === 'done'
                  ? 'border-green-400/50 bg-green-400/15 text-green-400'
                  : state === 'current'
                    ? 'border-primary/60 bg-primary/15 text-primary'
                    : 'border-border text-muted'
              }`}
            >
              {state === 'done' ? <Check size={11} /> : <Icon size={11} />}
            </span>
            <span className="min-w-0 truncate text-sm font-medium leading-tight">
              {phase.label}
            </span>
          </button>
        )
      })}
    </nav>
  )
}
