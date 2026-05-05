import { useAuth } from '@/hooks/useAuth'
import { Navigate } from 'react-router-dom'
import { motion } from 'framer-motion'
import Card from '@/components/ui/Card'
import LoginButton from '@/components/auth/LoginButton'
import { Shield, Swords } from 'lucide-react'

export default function Login() {
  const { isLoggedIn } = useAuth()

  if (isLoggedIn) return <Navigate to="/" replace />

  return (
    <div className="flex items-center justify-center min-h-[70vh]">
      <motion.div
        initial={{ opacity: 0, scale: 0.95 }}
        animate={{ opacity: 1, scale: 1 }}
        className="w-full max-w-md"
      >
        <Card className="p-10 text-center relative overflow-hidden border-primary/20 shadow-[0_0_50px_rgba(245,158,11,0.05)]">
          <div className="absolute top-0 left-0 w-full h-1 bg-gradient-to-r from-transparent via-primary/50 to-transparent" />

          <div className="mb-8">
            <div className="inline-flex p-4 rounded-lg bg-primary/10 border border-primary/20 mb-6">
              <Swords size={40} className="text-primary shadow-[var(--glow-primary)]" />
            </div>
            <h1 className="text-3xl font-bold text-foreground font-display tracking-widest uppercase">Identifikation</h1>
            <p className="text-muted italic mt-2 text-sm">
              "Wer bist du, der die Arena betreten will?"
            </p>
          </div>

          <div className="space-y-6">
            <p className="text-xs text-muted leading-relaxed uppercase tracking-wider">
              Verknüpfe dein Discord-Siegel, um dich für die kommenden Prüfungen zu rüsten.
            </p>

            <div className="py-4">
               <LoginButton />
            </div>

            <div className="pt-6 border-t border-white/5 space-y-2">
              <div className="flex items-center justify-center gap-2 text-[10px] text-muted font-bold uppercase tracking-[0.2em]">
                <Shield size={12} className="text-primary/50" />
                Sichere Verbindung
              </div>
              <p className="text-[10px] text-muted/50 italic">
                Wir erfassen nur dein Profil und deine Rollen.
              </p>
            </div>
          </div>
        </Card>
      </motion.div>
    </div>
  )
}
