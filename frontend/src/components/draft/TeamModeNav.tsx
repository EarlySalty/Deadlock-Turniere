import { NavLink } from 'react-router-dom'
import { Swords, Users } from 'lucide-react'

const EINTRAEGE = [
  { to: '/draft', label: 'Pick / Ban Draft', icon: Swords },
  { to: '/comp', label: 'Comp-Finder', icon: Users },
]

function linkKlassen(isActive: boolean) {
  return `inline-flex items-center gap-2 border px-5 py-2.5 text-sm font-semibold transition-colors ${isActive ? 'border-primary/50 bg-primary/10 text-primary' : 'border-border bg-card text-muted hover:text-foreground'}`
}

export default function TeamModeNav({ variante = 'standard' }: { variante?: 'standard' | 'pulp' }) {
  const pulp = variante === 'pulp'
  return (
    <nav
      aria-label="Team-Modus"
      className={pulp ? 'draft-mode-nav--pulp relative z-10' : 'relative z-10 mb-8 flex flex-wrap justify-center gap-2'}
    >
      {EINTRAEGE.map(({ to, label, icon: Icon }) => (
        <NavLink key={to} to={to} className={pulp ? undefined : ({ isActive }) => linkKlassen(isActive)}>
          <Icon size={16} />{label}
        </NavLink>
      ))}
    </nav>
  )
}
