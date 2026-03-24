import { useAuth } from '@/hooks/useAuth'
import { Navigate } from 'react-router-dom'
import Card from '@/components/ui/Card'
import LoginButton from '@/components/auth/LoginButton'
import { Shield } from 'lucide-react'

export default function Login() {
  const { isLoggedIn } = useAuth()

  if (isLoggedIn) return <Navigate to="/" replace />

  return (
    <div className="flex items-center justify-center min-h-[60vh]">
      <Card className="p-8 max-w-md w-full text-center">
        <Shield size={48} className="mx-auto text-primary mb-4" />
        <h1 className="text-2xl font-bold text-foreground mb-2">Anmelden</h1>
        <p className="text-muted mb-6">
          Melde dich mit deinem Discord-Account an, um dich fuer Turniere zu registrieren und Teams zu erstellen.
        </p>
        <LoginButton />
        <p className="text-xs text-muted mt-4">
          Wir benoetigen Zugriff auf dein Discord-Profil und deine Server-Rollen.
        </p>
      </Card>
    </div>
  )
}
