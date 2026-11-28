import { motion } from 'framer-motion'
import { BEREIT_GRUEN, teamFarbe } from './farben'

export default function TeamKarte({
  team,
  name,
  claimed,
  ready,
  duBistEs,
  gesperrt,
  onClaim,
  onReady,
  onLeave,
}: {
  team: 1 | 2
  name: string
  claimed: boolean
  ready: boolean
  duBistEs: boolean
  gesperrt: boolean
  onClaim: () => void
  onReady: () => void
  onLeave: () => void
}) {
  const farbe = teamFarbe(team)

  return (
    <motion.div
      whileHover={{ scale: 1.02 }}
      transition={{ type: 'spring', stiffness: 300, damping: 24 }}
      className={`draft-team-card relative flex min-h-[160px] w-full min-w-0 max-w-[250px] flex-col items-center justify-center gap-4 px-6 py-6 ${
        ready ? 'border-(--draft-gruen) pulse-glow-ready' : ''
      }`}
      style={{
        borderColor: ready ? undefined : claimed ? farbe : 'rgba(255,255,255,0.06)',
        borderStyle: claimed ? 'solid' : 'dashed',
      }}
    >
      <div
        className="max-w-full wrap-anywhere text-center text-sm font-black uppercase tracking-tight"
        style={{ color: claimed ? farbe : undefined }}
      >
        {name}
      </div>

      {!claimed && (
        <motion.button
          type="button"
          whileTap={{ scale: 0.95 }}
          onClick={onClaim}
          disabled={gesperrt}
          className="border border-dashed px-5 py-2.5 text-[10px] font-bold uppercase tracking-[0.15em] transition-opacity disabled:opacity-40"
          style={{ borderColor: farbe, color: farbe }}
        >
          Captain übernehmen
        </motion.button>
      )}

      {claimed && duBistEs && (
        <>
          <div className="flex items-center gap-1.5">
            <span
              className="h-1.5 w-1.5 rounded-full"
              style={{ backgroundColor: ready ? BEREIT_GRUEN : farbe }}
            />
            <span className="text-[10px] uppercase tracking-[0.15em] text-white/60">
              Du bist Captain
            </span>
          </div>
          <div className="flex items-center gap-3">
            <motion.button
              type="button"
              whileHover={ready ? undefined : { scale: 1.03 }}
              whileTap={{ scale: 0.95 }}
              onClick={onReady}
              disabled={ready || gesperrt}
              className={`px-5 py-2.5 text-[11px] font-bold uppercase tracking-[0.15em] ${
                ready ? 'bg-(--draft-gruen) text-(--draft-tinte)' : 'bg-white text-(--draft-tinte)'
              }`}
            >
              Bereit
            </motion.button>
            <button
              type="button"
              onClick={onLeave}
              className="text-[10px] uppercase tracking-[0.15em] text-white/30 transition-colors hover:text-white/70"
            >
              Verlassen
            </button>
          </div>
        </>
      )}

      {claimed && !duBistEs && (
        <>
          <div className="flex items-center gap-1.5">
            <span className="h-1.5 w-1.5 rounded-full bg-(--draft-gruen)" />
            <span className="text-[10px] uppercase tracking-[0.15em] text-white/60">
              Captain da
            </span>
          </div>
          {ready ? (
            <span className="text-[10px] uppercase tracking-[0.15em] text-(--draft-gruen)">
              Bereit
            </span>
          ) : (
            <motion.span
              animate={{ opacity: [0.4, 0.7, 0.4] }}
              transition={{ duration: 2, repeat: Infinity, ease: 'easeInOut' }}
              className="text-xs text-white/40"
            >
              Wartet...
            </motion.span>
          )}
        </>
      )}
    </motion.div>
  )
}
