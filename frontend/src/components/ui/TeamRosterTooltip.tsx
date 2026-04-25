import { useState, useRef, useId, type ReactNode } from 'react'
import { Crown, User } from 'lucide-react'
import type { TeamMemberPublic } from '@/types/tournament'

interface TeamRosterTooltipProps {
  children: ReactNode
  teamName: string
  members: TeamMemberPublic[]
  align?: 'left' | 'center' | 'right'
  disabled?: boolean
  /** Wrapper als Block statt inline-block — nötig wenn Children volle Breite einnehmen sollen. */
  block?: boolean
}

function avgRankScore(members: TeamMemberPublic[]): number {
  if (members.length === 0) return 0
  const sum = members.reduce((total, member) => total + (member.rank_score ?? 0), 0)
  return Math.round((sum / members.length) * 10) / 10
}

export default function TeamRosterTooltip({
  children,
  teamName,
  members,
  align = 'center',
  disabled = false,
  block = false,
}: TeamRosterTooltipProps) {
  const [open, setOpen] = useState(false)
  const tooltipId = useId()
  const closeTimer = useRef<number | null>(null)

  if (disabled || members.length === 0) {
    return <>{children}</>
  }

  const captain = members.find((member) => member.role === 'captain') ?? null
  const others = members.filter((member) => member.role !== 'captain')
  const avg = avgRankScore(members)

  const show = () => {
    if (closeTimer.current) {
      window.clearTimeout(closeTimer.current)
      closeTimer.current = null
    }
    setOpen(true)
  }

  const hide = () => {
    closeTimer.current = window.setTimeout(() => setOpen(false), 80)
  }

  const alignClass =
    align === 'left' ? 'left-0' : align === 'right' ? 'right-0' : 'left-1/2 -translate-x-1/2'

  return (
    <span
      className={`relative ${block ? 'block' : 'inline-block'}`}
      onMouseEnter={show}
      onMouseLeave={hide}
      onFocus={show}
      onBlur={hide}
      aria-describedby={open ? tooltipId : undefined}
    >
      {children}

      {open && (
        <span
          id={tooltipId}
          role="tooltip"
          className={`absolute z-30 mt-2 ${alignClass} top-full w-64 rounded-lg border border-border bg-card p-3 shadow-xl shadow-black/40 text-xs text-foreground`}
        >
          <div className="flex items-center justify-between gap-2 border-b border-border/60 pb-2">
            <span className="truncate font-semibold text-foreground">{teamName}</span>
            <span className="text-[10px] text-muted">Ø {avg}</span>
          </div>

          <ul className="mt-2 space-y-1">
            {captain && (
              <li className="flex items-center gap-2">
                <Crown size={12} className="text-amber-400 shrink-0" />
                <span className="truncate font-medium text-foreground">
                  {captain.discord_name ?? 'Unbekannter Captain'}
                </span>
                {captain.rank && (
                  <span className="ml-auto text-[10px] uppercase tracking-wide text-muted">
                    {captain.rank}
                  </span>
                )}
              </li>
            )}

            {others.map((member, index) => (
              <li
                key={member.id ?? `${member.discord_name}-${index}`}
                className="flex items-center gap-2"
              >
                <User size={12} className="text-muted shrink-0" />
                <span className="truncate text-foreground/90">
                  {member.discord_name ?? '—'}
                </span>
                {member.rank && (
                  <span className="ml-auto text-[10px] uppercase tracking-wide text-muted">
                    {member.rank}
                  </span>
                )}
              </li>
            ))}
          </ul>
        </span>
      )}
    </span>
  )
}
