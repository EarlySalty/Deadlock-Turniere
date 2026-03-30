import { Link } from 'react-router-dom'
import { useAuth } from '@/hooks/useAuth'
import LoginButton from '@/components/auth/LoginButton'
import Button from '@/components/ui/Button'
import { Trophy, LogOut, Settings } from 'lucide-react'

export default function Header() {
  const { user, isLoggedIn, logout } = useAuth()

  return (
    <header className="border-b border-border bg-card/80 backdrop-blur-sm sticky top-0 z-50">
      <div className="mx-auto max-w-7xl px-4 sm:px-6 lg:px-8">
        <div className="flex h-16 items-center justify-between">
          {/* Logo + Nav */}
          <div className="flex items-center gap-8">
            <Link to="/" className="flex items-center gap-2 text-lg font-bold text-foreground">
              <Trophy size={24} className="text-primary" />
              Deadlock Turniere
            </Link>
            <nav className="hidden sm:flex items-center gap-4">
              <Link to="/" className="text-sm text-muted hover:text-foreground transition-colors">
                Startseite
              </Link>
              <Link to="/hilfe" className="text-sm text-muted hover:text-foreground transition-colors">
                Hilfe
              </Link>
            </nav>
          </div>

          {/* Right side */}
          <div className="flex items-center gap-3">
            {isLoggedIn && user ? (
              <>
                {(user.is_mod || user.is_admin) && (
                  <Link to="/admin">
                    <Button variant="ghost" size="sm">
                      <Settings size={16} />
                      <span className="hidden sm:inline">Admin</span>
                    </Button>
                  </Link>
                )}
                <div className="flex items-center gap-2">
                  {user.discord_avatar ? (
                    <img
                      src={user.discord_avatar}
                      alt={user.discord_name}
                      className="w-8 h-8 rounded-full"
                    />
                  ) : (
                    <div className="w-8 h-8 rounded-full bg-primary/20 flex items-center justify-center text-sm font-medium text-primary">
                      {user.discord_name.charAt(0).toUpperCase()}
                    </div>
                  )}
                  <span className="hidden sm:inline text-sm text-foreground">{user.discord_name}</span>
                </div>
                <Button variant="ghost" size="sm" onClick={logout}>
                  <LogOut size={16} />
                </Button>
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
