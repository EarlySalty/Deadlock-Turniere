import { Link, useLocation } from 'react-router-dom'
import { useQuery } from '@tanstack/react-query'
import { useAuth } from '@/hooks/useAuth'
import { fetchMyProfile } from '@/api/client'
import LoginButton from '@/components/auth/LoginButton'
import Button from '@/components/ui/Button'
import { Trophy, LogOut, Settings, BarChart2, User, Swords } from 'lucide-react'

export default function Header() {
  const { user, isLoggedIn, logout } = useAuth()
  const location = useLocation()
  const { data: myProfile } = useQuery({
    queryKey: ['profile', 'me'],
    queryFn: fetchMyProfile,
    enabled: isLoggedIn,
    staleTime: 5 * 60_000,
  })
  const avatarUrl = myProfile?.avatar_filename
    ? `/turnier/api/avatars/${user?.discord_id}`
    : (user?.discord_avatar ?? null)

  const navItems = [
    { label: 'Arena', path: '/', icon: Swords },
    { label: 'Rangliste', path: '/rangliste', icon: BarChart2 },
    { label: 'Kodex', path: '/hilfe', icon: Trophy },
  ]

  return (
    <header className="border-b border-white/5 bg-background/80 backdrop-blur-md sticky top-0 z-50">
      <div className="mx-auto max-w-7xl px-4 sm:px-6 lg:px-8">
        <div className="flex h-20 items-center justify-between">
          {/* Logo + Nav */}
          <div className="flex items-center gap-12">
            <Link to="/" className="flex items-center gap-3 text-xl font-bold text-foreground font-display tracking-widest group">
              <div className="p-2 rounded-lg bg-primary/10 border border-primary/20 group-hover:bg-primary/20 transition-colors shadow-[var(--glow-primary)]">
                <Trophy size={20} className="text-primary" />
              </div>
              Deadlock
            </Link>
            <nav className="hidden md:flex items-center gap-8">
              {navItems.map((item) => {
                const Icon = item.icon
                const isActive = location.pathname === item.path
                return (
                  <Link
                    key={item.path}
                    to={item.path}
                    className={`flex items-center gap-2 text-xs font-bold uppercase tracking-[0.2em] transition-all hover:text-primary ${
                      isActive ? 'text-primary' : 'text-muted'
                    }`}
                  >
                    <Icon size={14} />
                    {item.label}
                  </Link>
                )
              })}
            </nav>
          </div>

          {/* Right side */}
          <div className="flex items-center gap-4">
            {isLoggedIn && user ? (
              <>
                {(user.is_mod || user.is_admin) && (
                  <Link to="/admin">
                    <Button variant="ghost" size="sm" className="hidden sm:flex">
                      <Settings size={14} />
                      Admin
                    </Button>
                  </Link>
                )}
                <Link to="/profil" className="flex items-center gap-3 p-1.5 pr-4 rounded-lg border border-white/5 bg-white/5 hover:bg-white/10 transition-all group">
                  {avatarUrl ? (
                    <img
                      src={avatarUrl}
                      alt={user.discord_name}
                      className="w-8 h-8 rounded-lg grayscale group-hover:grayscale-0 transition-all border border-white/10"
                    />
                  ) : (
                    <div className="w-8 h-8 rounded-lg bg-primary/20 flex items-center justify-center text-primary border border-primary/20">
                      <User size={16} />
                    </div>
                  )}
                  <div className="hidden sm:block">
                    <p className="text-[10px] uppercase font-bold text-muted tracking-tighter leading-none mb-1">Spieler</p>
                    <p className="text-xs font-bold text-foreground tracking-wide leading-none">{user.discord_name}</p>
                  </div>
                </Link>
                <button
                  onClick={logout}
                  className="p-2 text-muted hover:text-danger transition-colors"
                  title="Abmelden"
                >
                  <LogOut size={18} />
                </button>
              </>
            ) : (
              <LoginButton />
            )}
          </div>
        </div>
      </div>
    </header>
  )
}
