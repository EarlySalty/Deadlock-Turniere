import { motion } from 'framer-motion'
import type { DraftAktion } from '@/types/draft'
import { TINTE, teamFarbe } from './farben'

export default function Endkarten({
  team,
  name,
  picks,
  splashQuellen,
}: {
  team: 1 | 2
  name: string
  picks: DraftAktion[]
  splashQuellen: Map<string, string>
}) {
  const farbe = teamFarbe(team)

  return (
    <div className="flex w-full max-w-[640px] flex-col gap-3">
      <div
        className="border-l-4 pl-3 text-[11px] font-black uppercase tracking-[0.25em]"
        style={{ color: farbe, borderColor: farbe }}
      >
        {name}
      </div>
      <div className="grid grid-cols-2 gap-3 sm:grid-cols-3">
        {Array.from({ length: 6 }).map((_, i) => {
          const pick: DraftAktion | undefined = picks[i]
          const splash = pick ? splashQuellen.get(pick.hero_name) : undefined
          return (
            <motion.div
              key={pick ? `${pick.hero_name}-${i}` : i}
              initial={{ opacity: 0, y: 50, scale: 0.8 }}
              animate={{ opacity: 1, y: 0, scale: 1 }}
              transition={{ delay: 0.4 + i * 0.08, duration: 0.7, ease: 'easeOut' }}
              whileHover={{ scale: 1.06 }}
              className="relative h-[220px] overflow-hidden rounded-xl border"
              style={{ borderColor: `${farbe}88`, boxShadow: `0 0 40px ${farbe}26` }}
            >
              {pick && splash ? (
                <>
                  <img
                    src={splash}
                    alt={pick.hero_name}
                    className="h-full w-full object-cover object-top"
                  />
                  <div className="absolute inset-x-0 bottom-0 h-2/3 bg-gradient-to-t from-black/85 to-transparent" />
                  <span
                    className="absolute left-2 top-2 flex h-5 w-5 items-center justify-center rounded-md text-[9px] font-black"
                    style={{ backgroundColor: farbe, color: TINTE }}
                  >
                    {i + 1}
                  </span>
                  <span className="absolute inset-x-2 bottom-2 truncate text-sm font-bold text-white">
                    {pick.hero_name}
                  </span>
                  {pick.is_auto && (
                    <span className="absolute right-2 top-2 rounded bg-black/60 px-1.5 py-0.5 text-[8px] font-bold uppercase tracking-wider text-white/60">
                      Auto
                    </span>
                  )}
                </>
              ) : (
                <div className="flex h-full items-center justify-center text-[10px] uppercase tracking-[0.2em] text-white/20">
                  Offen
                </div>
              )}
            </motion.div>
          )
        })}
      </div>
    </div>
  )
}
