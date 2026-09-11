import { motion } from 'framer-motion'
import type { DraftAktion } from '@/types/draft'
import { teamFarbe } from './farben'

export default function TeamSpalte({
  team,
  name,
  picks,
  aktiverSlot,
  portraits,
}: {
  team: 1 | 2
  name: string
  picks: DraftAktion[]
  aktiverSlot: number | null
  portraits: Map<string, string>
}) {
  const farbe = teamFarbe(team)
  const vonLinks = team === 1

  return (
    <div className="flex w-full flex-col gap-2">
      <div
        className="border-b pb-1.5 text-[11px] font-black uppercase tracking-[0.25em]"
        style={{ color: farbe, borderColor: `${farbe}66` }}
      >
        {name}
      </div>
      {Array.from({ length: 6 }).map((_, i) => {
        const pick: DraftAktion | undefined = picks[i]
        const aktiv = aktiverSlot === i
        const portrait = pick ? portraits.get(pick.hero_name) : undefined
        return (
          <div
            key={i}
            className={`relative flex h-12 items-center gap-2.5 overflow-hidden rounded-lg border px-2.5 transition-all duration-300 ${
              aktiv ? 'draft-slot-pulse' : ''
            }`}
            style={{
              backgroundColor: 'rgba(255,255,255,0.02)',
              borderColor: aktiv ? farbe : 'rgba(255,255,255,0.06)',
            }}
          >
            {!pick && (
              <span className="text-[10px] text-white/20">{i + 1}</span>
            )}
            {pick && (
              <>
                {portrait && (
                  <motion.img
                    key={pick.hero_name}
                    src={portrait}
                    alt={pick.hero_name}
                    initial={{ opacity: 0, x: vonLinks ? -25 : 25 }}
                    animate={{ opacity: 1, x: 0 }}
                    transition={{ duration: 0.35, ease: 'easeOut' }}
                    className="h-9 w-9 rounded object-cover"
                  />
                )}
                <motion.span
                  initial={{ opacity: 0, y: 8 }}
                  animate={{ opacity: 1, y: 0 }}
                  transition={{ delay: 0.1, duration: 0.3 }}
                  className="truncate text-xs font-semibold text-white/90"
                >
                  {pick.hero_name}
                </motion.span>
                {pick.is_auto && (
                  <span className="absolute right-2 top-1.5 rounded bg-white/10 px-1.5 py-0.5 text-[8px] font-bold uppercase tracking-wider text-white/50">
                    Auto
                  </span>
                )}
              </>
            )}
          </div>
        )
      })}
    </div>
  )
}
