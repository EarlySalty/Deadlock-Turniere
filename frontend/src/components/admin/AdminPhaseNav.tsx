import {
  CalendarRange,
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

interface PhaseConfig {
  id: AdminPhase
  label: string
  icon: LucideIcon
  /** Hint why this phase is hidden/disabled. */
  description: string
  /** Phase wird sichtbar wenn das Tournament in einem dieser Stadien ist (oder bestimmte Daten existieren). */
  isAvailable: (status: TournamentStatus, hasGroups: boolean, hasBracket: boolean) => boolean
}

const PHASE_CONFIG: PhaseConfig[] = [
  {
    id: 'setup',
    label: 'Setup',
    icon: Settings,
    description: 'Stammdaten, Zeitplan, Spielmodus',
    isAvailable: () => true,
  },
  {
    id: 'participants',
    label: 'Teilnehmer',
    icon: Users,
    description: 'Spieler, Teams, Solo-Pool',
    isAvailable: (status) =>
      ['draft', 'registration', 'checkin', 'group_phase', 'bracket'].includes(status),
  },
  {
    id: 'checkin',
    label: 'Check-in',
    icon: CheckCheck,
    description: 'Anwesenheit prüfen, Bracket starten',
    isAvailable: (status) => status === 'checkin',
  },
  {
    id: 'group_phase',
    label: 'Gruppenphase',
    icon: LayoutGrid,
    description: 'Gruppen-Matches & Tabellen',
    isAvailable: (_status, hasGroups) => hasGroups,
  },
  {
    id: 'bracket',
    label: 'Bracket',
    icon: GitBranch,
    description: 'KO-Runden & Match-Steuerung',
    isAvailable: (_status, _hasGroups, hasBracket) => hasBracket,
  },
  {
    id: 'voice',
    label: 'Voice & Caster',
    icon: Headphones,
    description: 'Voice-Channel-Splits & Casting',
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
      aria-label="Turnier-Phasen"
      className="flex flex-col gap-1 rounded-xl border border-border bg-card p-2"
    >
      {visible.map((phase) => {
        const Icon = phase.icon
        const isActive = activePhase === phase.id
        return (
          <button
            key={phase.id}
            type="button"
            role="tab"
            aria-selected={isActive}
            onClick={() => onChange(phase.id)}
            className={`group flex items-start gap-3 rounded-lg px-3 py-2.5 text-left transition-colors ${
              isActive
                ? 'bg-primary/15 text-primary'
                : 'text-foreground/85 hover:bg-card-hover hover:text-foreground'
            }`}
          >
            <Icon
              size={16}
              className={`mt-0.5 shrink-0 ${isActive ? 'text-primary' : 'text-muted group-hover:text-foreground'}`}
            />
            <div className="min-w-0">
              <div className="text-sm font-medium leading-tight">{phase.label}</div>
              <div
                className={`mt-0.5 truncate text-[11px] ${
                  isActive ? 'text-primary/70' : 'text-muted'
                }`}
              >
                {phase.description}
              </div>
            </div>
          </button>
        )
      })}

      <div className="mt-2 border-t border-border/60 pt-2 px-3 pb-1 text-[10px] uppercase tracking-wider text-muted">
        <CalendarRange size={10} className="mr-1 inline" />
        Aktueller Status: {status}
      </div>
    </nav>
  )
}
