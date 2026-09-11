import { motion } from 'framer-motion'
import type { DraftHero } from '@/types/draft'
import { teamFarbe } from './farben'

export default function PickAnsage({
  held,
  team,
  teamName,
}: {
  held: DraftHero
  team: 1 | 2
  teamName: string
}) {
  const farbe = teamFarbe(team)
  const partikel = Array.from({ length: 12 })

  return (
    <motion.div
      initial={{ opacity: 0 }}
      animate={{ opacity: 1 }}
      exit={{ opacity: 0 }}
      transition={{ duration: 0.3 }}
      className="pointer-events-none absolute inset-0 z-30 flex items-center justify-center overflow-hidden"
    >
      <div className="relative">
        <div className="absolute left-1/2 top-1/2">
          {partikel.map((_, n) => {
            const i = 60 + (n / 12) * 80
            const winkel = (n / 12) * Math.PI * 2
            return (
              <motion.span
                key={n}
                initial={{ opacity: 0 }}
                animate={{
                  opacity: [0, 1, 0],
                  scale: [0, 1.2, 0],
                  x: Math.cos(winkel) * i,
                  y: Math.sin(winkel) * i - 50,
                }}
                transition={{ duration: 0.7, ease: 'easeOut', delay: 0.15 }}
                className="absolute h-1.5 w-1.5 rounded-full"
                style={{ backgroundColor: farbe }}
              />
            )
          })}
        </div>
        <div className="relative text-center">
          <div className="text-[10px] uppercase tracking-[0.3em]" style={{ color: farbe }}>
            {teamName} wählt
          </div>
          <motion.div
            initial={{ scale: 1.8, opacity: 0 }}
            animate={{ scale: 1, opacity: 1 }}
            transition={{ duration: 0.45, ease: 'easeOut' }}
            className="mt-3 font-display text-6xl font-black uppercase tracking-tight"
            style={{ color: farbe }}
          >
            {held.name}
          </motion.div>
        </div>
      </div>
    </motion.div>
  )
}
