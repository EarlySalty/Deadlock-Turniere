import { motion } from 'framer-motion'
import type { DraftHero } from '@/types/draft'
import { heroCardImageUrl } from '@/hooks/draftLobbyState'

export default function BanStempel({ held }: { held: DraftHero }) {
  const splash = heroCardImageUrl(held.card_image_url, held.image_url)

  return (
    <motion.div
      initial={{ opacity: 0 }}
      animate={{ opacity: 1 }}
      exit={{ opacity: 0 }}
      transition={{ duration: 0.3 }}
      className="absolute inset-0 z-30 overflow-hidden bg-[#160404]/85"
    >
      <div className="absolute left-[-25%] top-1/2 h-px w-[150%] rotate-[14deg] bg-[#ef4444]/20" />
      <div className="absolute left-[-25%] top-1/2 h-px w-[150%] -rotate-[10deg] bg-[#ef4444]/20" />
      <div className="absolute left-1/2 top-1/2 h-40 w-40 -translate-x-1/2 -translate-y-1/2 rounded-full bg-[#ef4444]/30 blur-3xl" />

      {splash && (
        <div className="absolute inset-y-0 right-[6%] w-[44%] opacity-40">
          <img
            src={splash}
            alt=""
            className="h-full w-full object-cover object-top grayscale"
          />
          <div className="absolute inset-0 bg-gradient-to-l from-[#ef4444]/40 to-transparent" />
        </div>
      )}

      <div className="absolute left-1/2 top-[42%] -translate-x-1/2 -translate-y-1/2">
        <motion.div
          initial={{ scale: 0, opacity: 0.4 }}
          animate={{ scale: [0, 7], opacity: [0.4, 0] }}
          transition={{ duration: 1.8, ease: 'easeOut' }}
          className="h-40 w-40 rounded-full bg-[#ef4444]/20"
        />
      </div>

      <div className="absolute inset-0 flex items-center justify-center">
        <div className="relative flex h-64 w-64 items-center justify-center">
          <motion.div
            animate={{
              borderColor: [
                'rgba(239,68,68,0.25)',
                'rgba(239,68,68,0.55)',
                'rgba(239,68,68,0.25)',
              ],
            }}
            transition={{ duration: 1.5, repeat: Infinity }}
            className="absolute inset-0 rounded-full border-2 border-[#ef4444]/25"
          />
          <svg viewBox="0 0 100 100" className="h-44 w-44" aria-hidden="true">
            <motion.path
              d="M18 18 L82 82"
              stroke="#ef4444"
              strokeWidth={6}
              strokeLinecap="round"
              fill="none"
              initial={{ pathLength: 0 }}
              animate={{ pathLength: 1 }}
              transition={{ duration: 0.4, ease: 'easeOut' }}
            />
            <motion.path
              d="M82 18 L18 82"
              stroke="#ef4444"
              strokeWidth={6}
              strokeLinecap="round"
              fill="none"
              initial={{ pathLength: 0 }}
              animate={{ pathLength: 1 }}
              transition={{ delay: 0.15, duration: 0.4, ease: 'easeOut' }}
            />
          </svg>
        </div>
      </div>

      <div className="absolute left-1/2 top-[64%] -translate-x-1/2 -translate-y-1/2">
        <motion.div
          initial={{ opacity: 0, scale: 0.6, rotate: -10 }}
          animate={{ opacity: 1, scale: 1, rotate: 0 }}
          transition={{ delay: 0.3, type: 'spring', stiffness: 200 }}
          className="border-4 border-[#ef4444] bg-[#450a0a]/90 px-8 py-3 backdrop-blur-md drop-shadow-[0_0_60px_rgba(239,68,68,0.9)]"
        >
          <span className="text-5xl font-black uppercase tracking-[0.3em] text-[#ef4444]">
            Gebannt
          </span>
        </motion.div>
      </div>

      <div className="absolute bottom-[12%] left-[6%]">
        <div className="text-[10px] uppercase tracking-[0.3em] text-[#ef4444]">
          Eliminiert
        </div>
        <div className="mt-1 font-display text-3xl font-black uppercase text-white/90 underline decoration-[#ef4444] decoration-4 underline-offset-8">
          {held.name}
        </div>
      </div>
    </motion.div>
  )
}
