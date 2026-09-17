import { NavLink } from 'react-router-dom'
import { Swords, Users } from 'lucide-react'

export default function TeamModeNav() {
  return (
    <nav aria-label="Team-Modus" className="relative z-10 mb-8 flex flex-wrap justify-center gap-2">
      {[
        { to: '/draft', label: 'Pick / Ban Draft', icon: Swords },
        { to: '/comp', label: 'Comp-Finder', icon: Users },
      ].map(({ to, label, icon: Icon }) => (
        <NavLink key={to} to={to} className={({ isActive }) => `inline-flex items-center gap-2 rounded-full border px-5 py-2.5 text-sm font-semibold transition-colors ${isActive ? 'border-primary/50 bg-primary/10 text-primary' : 'border-border bg-card text-muted hover:text-foreground'}`}>
          <Icon size={16} />{label}
        </NavLink>
      ))}
    </nav>
  )
}
