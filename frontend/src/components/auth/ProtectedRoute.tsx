import type { ReactNode } from 'react'
import { Navigate } from 'react-router-dom'
import { useAuth } from '@/hooks/useAuth'
import LoadingSpinner from '@/components/ui/LoadingSpinner'
import Card from '@/components/ui/Card'
import { ShieldX } from 'lucide-react'

interface ProtectedRouteProps {
  children: ReactNode
  requireMod?: boolean
  requireAdmin?: boolean
}

export default function ProtectedRoute({ children, requireMod, requireAdmin }: ProtectedRouteProps) {
  const { user, isLoading, isLoggedIn } = useAuth()

  if (isLoading) {
    return <LoadingSpinner className="py-20" size={32} />
  }

  if (!isLoggedIn) {
    return <Navigate to="/login" replace />
  }

  if (requireAdmin && !user?.is_admin) {
    return (
      <div className="flex items-center justify-center py-20">
        <Card className="max-w-md text-center">
          <ShieldX size={48} className="mx-auto mb-4 text-danger" />
          <h2 className="text-xl font-bold mb-2">Kein Zugriff</h2>
          <p className="text-muted">Du benoetigst Admin-Rechte fuer diesen Bereich.</p>
        </Card>
      </div>
    )
  }

  if (requireMod && !user?.is_mod && !user?.is_admin) {
    return (
      <div className="flex items-center justify-center py-20">
        <Card className="max-w-md text-center">
          <ShieldX size={48} className="mx-auto mb-4 text-danger" />
          <h2 className="text-xl font-bold mb-2">Kein Zugriff</h2>
          <p className="text-muted">Du benoetigst Moderator-Rechte fuer diesen Bereich.</p>
        </Card>
      </div>
    )
  }

  return <>{children}</>
}
