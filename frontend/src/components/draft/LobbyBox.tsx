import { motion } from 'framer-motion'
import type { DraftLobbyInfo } from '@/types/draft'
import KopierKnopf from './KopierKnopf'

export default function LobbyBox({
  lobby,
  onRetry,
  retryLaeuft,
}: {
  lobby: DraftLobbyInfo
  onRetry: () => void
  retryLaeuft: boolean
}) {
  if (lobby.status === 'keine' || lobby.status === 'angefordert') {
    return (
      <div className="flex items-center justify-center gap-3 rounded-xl border border-white/[0.08] bg-white/[0.03] px-6 py-4">
        <span className="h-5 w-5 animate-spin rounded-full border-2 border-[#c8a86b]/30 border-t-[#c8a86b]" />
        <span className="text-xs uppercase tracking-[0.25em] text-white/50">
          Lobby wird erstellt...
        </span>
      </div>
    )
  }

  if (lobby.status === 'fehler') {
    return (
      <div className="flex flex-col items-center gap-3 rounded-xl border border-[#ef4444]/30 bg-[#ef4444]/[0.06] px-6 py-4">
        <span className="text-xs text-[#ef4444]">
          Lobby konnte nicht erstellt werden, bitte selbst anlegen
        </span>
        <button
          type="button"
          onClick={onRetry}
          disabled={retryLaeuft}
          className="rounded-lg border border-white/15 px-4 py-2 text-[10px] font-bold uppercase tracking-[0.2em] text-white/70 transition-colors hover:text-white disabled:opacity-50"
        >
          Erneut versuchen
        </button>
      </div>
    )
  }

  return (
    <motion.div
      initial={{ opacity: 0, scale: 0.9, y: 20 }}
      animate={{ opacity: 1, scale: 1, y: 0 }}
      transition={{ type: 'spring', stiffness: 200 }}
      className="flex flex-wrap items-center justify-center gap-x-5 gap-y-3 rounded-xl border border-[#c8a86b]/30 bg-[#c8a86b]/[0.05] px-6 py-4"
    >
      <span className="flex items-baseline gap-3">
        <span className="text-[10px] uppercase tracking-[0.3em] text-white/40">Lobby</span>
        <span className="font-mono text-2xl font-bold tracking-[0.3em] text-white">
          {lobby.join_code ?? 'XXXXXX'}
        </span>
      </span>
      {lobby.join_code && (
        <KopierKnopf text={lobby.join_code} label="Code kopieren" variante="gold" pulse />
      )}
    </motion.div>
  )
}
