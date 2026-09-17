import { useState } from 'react'
import { Link, useLocation } from 'react-router-dom'
import { useQuery } from '@tanstack/react-query'
import { useAuth } from '@/hooks/useAuth'
import { fetchMyProfile } from '@/api/client'
import LoginButton from '@/components/auth/LoginButton'
import Button from '@/components/ui/Button'
import { Trophy, LogOut, Settings, BarChart2, User, Swords, Users, Menu, X } from 'lucide-react'

const navItems = [
  { label: 'Arena', path: '/', icon: Swords },
  { label: 'Draft', path: '/draft', icon: Swords },
  { label: 'Comp-Finder', path: '/comp', icon: Users },
  { label: 'Draft', path: '/draft', icon: Swords },
  { label: 'Comp-Finder', path: '/comp', icon: Users },
  { label: 'Rangliste', path: '/rangliste', icon: BarChart2 },
  { label: 'Regelwerk', path: '/hilfe', icon: Trophy },
]

export default function Header() {
  const { user, isLoggedIn, logout } = useAuth()
  const location = useLocation()
  const [menuOpen, setMenuOpen] = useState(false)
  const { data: myProfile } = useQuery({
    queryKey: ['profile', 'me'],
    queryFn: fetchMyProfile,
    enabled: isLoggedIn,
    staleTime: 5 * 60_000,
  })
  const avatarUrl = myProfile?.avatar_filename
    ? `/turnier/api/avatars/${user?.discord_id}`
    : (user?.discord_avatar ?? null)

  return (
    <header className="sticky top-0 z-50 border-b border-border bg-background/90 backdrop-blur-xl">
      <div className="mx-auto max-w-[1800px] px-4 sm:px-6">
        <div className="flex h-16 items-center gap-4">
          <Link
            to="/"
            className="mr-auto flex min-w-0 items-center gap-2.5 select-none"
            aria-label="Deutsche Deadlock Community – Turniere"
            onClick={() => setMenuOpen(false)}
          >
            <img
              src={`${import.meta.env.BASE_URL}brand/deadlock-d-logo.png`}
              alt=""
              className="h-8 w-8 shrink-0"
            />
            <span className="min-w-0">
              <span className="block truncate bg-gradient-to-r from-primary to-accent bg-clip-text font-display text-sm font-bold leading-tight text-transparent sm:text-lg">
                Deutsche Deadlock Community
              </span>
              <span className="hidden text-[10px] font-bold uppercase tracking-[0.22em] text-muted sm:block">
                Turniere
              </span>
            </span>
          </Link>

          <nav className="hidden items-center gap-5 lg:flex" aria-label="Turnier-Navigation">
            {navItems.map((item) => {
              const Icon = item.icon
              const isActive = location.pathname === item.path || (item.path !== '/' && location.pathname.startsWith(`${item.path}/`)) || (item.path !== '/' && location.pathname.startsWith(`${item.path}/`))
              return (
                <Link
                  key={item.path}
                  to={item.path}
                  className={`flex items-center gap-2 text-sm font-medium transition-colors ${
                    isActive ? 'text-foreground' : 'text-muted hover:text-foreground'
                  }`}
                >
                  <Icon size={15} className={isActive ? 'text-primary' : ''} />
                  {item.label}
                </Link>
              )
            })}
          </nav>

          <div className="flex items-center gap-2 sm:gap-3">
            {isLoggedIn && user ? (
              <>
                {(user.is_mod || user.is_admin) && (
                  <Link to="/admin" className="hidden sm:block">
                    <Button variant="ghost" size="sm">
                      <Settings size={15} />
                      Admin
                    </Button>
                  </Link>
                )}
                <Link
                  to="/profil"
                  className="inline-flex items-center gap-2 rounded-lg border border-border bg-card px-2.5 py-2 text-sm font-semibold transition-colors hover:border-border-hover hover:bg-card-hover sm:px-3"
                >
                  {avatarUrl ? (
                    <img
                      src={avatarUrl}
                      alt={user.discord_name}
                      className="h-6 w-6 rounded-md border border-border object-cover"
                    />
                  ) : (
                    <span className="flex h-6 w-6 items-center justify-center rounded-md bg-primary/15 text-primary">
                      <User size={14} />
                    </span>
                  )}
                  <span className="hidden max-w-32 truncate sm:block">{user.discord_name}</span>
                </Link>
                <button
                  onClick={logout}
                  className="hidden p-2 text-muted transition-colors hover:text-danger sm:block"
                  title="Abmelden"
                  aria-label="Abmelden"
                >
                  <LogOut size={18} />
                </button>
              </>
            ) : (
              <LoginButton className="hidden sm:inline-flex" />
            )}

            <button
              type="button"
              className="rounded-lg p-2 text-muted transition-colors hover:bg-card hover:text-foreground lg:hidden"
              onClick={() => setMenuOpen((open) => !open)}
              aria-label={menuOpen ? 'Navigation schließen' : 'Navigation öffnen'}
              aria-expanded={menuOpen}
              aria-controls="turnier-mobile-navigation"
            >
              {menuOpen ? <X size={21} /> : <Menu size={21} />}
            </button>
          </div>
        </div>
      </div>

      {menuOpen && (
        <nav id="turnier-mobile-navigation" className="border-t border-border bg-background/95 lg:hidden" aria-label="Turnier-Navigation mobil">
          <div className="mx-auto flex max-w-7xl flex-col gap-1 px-4 py-3 sm:px-6">
            {!isLoggedIn && <LoginButton className="mb-2 justify-center sm:hidden" />}
            {navItems.map((item) => {
              const Icon = item.icon
              const isActive = location.pathname === item.path || (item.path !== '/' && location.pathname.startsWith(`${item.path}/`)) || (item.path !== '/' && location.pathname.startsWith(`${item.path}/`))
              return (
                <Link
                  key={item.path}
                  to={item.path}
                  onClick={() => setMenuOpen(false)}
                  className={`flex items-center gap-3 rounded-lg px-3 py-2.5 text-sm font-medium transition-colors ${
                    isActive ? 'bg-primary/10 text-foreground' : 'text-muted hover:bg-card hover:text-foreground'
                  }`}
                >
                  <Icon size={16} className={isActive ? 'text-primary' : ''} />
                  {item.label}
                </Link>
              )
            })}
            {isLoggedIn && user && (
              <button
                onClick={logout}
                className="mt-2 flex items-center gap-3 border-t border-border px-3 pt-3 text-left text-sm font-medium text-muted hover:text-danger sm:hidden"
              >
                <LogOut size={16} />
                Abmelden
              </button>
            )}
          </div>
        </nav>
      )}
    </header>
  )
}
