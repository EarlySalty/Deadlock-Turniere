import { motion } from 'framer-motion'
import type { DraftHero } from '@/types/draft'
import { heroCardImageUrl } from '@/hooks/draftLobbyState'
import { ROT, teamFarbe } from './farben'

export default function SplashBuehne({
  held,
  team,
  teamName,
  istBan,
  kannBestaetigen,
  beschaeftigt,
  onBestaetigen,
}: {
  held: DraftHero
  team: 1 | 2
  teamName: string
  istBan: boolean
  kannBestaetigen: boolean
  beschaeftigt: boolean
  onBestaetigen: () => void
}) {
  const farbe = istBan ? ROT : teamFarbe(team)
  const splash = heroCardImageUrl(held.card_image_url, held.image_url)
  const links = team === 1

  return (
    <div className="pointer-events-none absolute inset-0 z-10">
      {splash && (
        <motion.div
          key={held.name}
          initial={{ opacity: 0, x: links ? -80 : 80 }}
          animate={{ opacity: 1, x: 0 }}
          transition={{ duration: 0.6, ease: [0.16, 1, 0.3, 1] }}
          className={`absolute inset-y-0 w-[44%] ${links ? 'left-[6%]' : 'right-[6%]'}`}
          style={{
            WebkitMaskImage: `linear-gradient(to ${links ? 'right' : 'left'}, black 55%, transparent 100%)`,
            maskImage: `linear-gradient(to ${links ? 'right' : 'left'}, black 55%, transparent 100%)`,
          }}
        >
          <img src={splash} alt="" className="h-full w-full object-cover object-top" />
          <div
            className="absolute inset-0"
            style={{
              background: `linear-gradient(to ${links ? 'right' : 'left'}, ${farbe}30, transparent 70%)`,
            }}
          />
        </motion.div>
      )}

      <motion.div
        key={`name-${held.name}`}
        initial={{ opacity: 0, letterSpacing: '-.04em', y: 28 }}
        animate={{ opacity: 1, letterSpacing: 0, y: 0 }}
        transition={{ duration: 1, ease: [0.2, 0.8, 0.2, 1] }}
        className="absolute bottom-[22%] left-[6%] max-w-[36%]"
      >
        <div
          className="flex items-center gap-2 text-[10px] uppercase tracking-[0.25em]"
          style={{ color: farbe }}
        >
          <span className="h-1.5 w-1.5 rounded-full" style={{ backgroundColor: farbe }} />
          <span>{istBan ? 'Bannen' : `${teamName} wählt`}</span>
        </div>
        <div className="mt-2 font-display text-4xl font-black uppercase leading-none tracking-tight text-white xl:text-5xl">
          {held.name}
        </div>
      </motion.div>

      {kannBestaetigen && (
        <div className="pointer-events-auto absolute left-1/2 top-[58%] -translate-x-1/2 -translate-y-1/2">
          <motion.button
            type="button"
            whileTap={{ scale: 0.95 }}
            disabled={beschaeftigt}
            onClick={onBestaetigen}
            className={`w-[180px] rounded-lg border-2 py-3 text-xs font-black uppercase tracking-[0.25em] backdrop-blur-sm disabled:opacity-50 ${
              istBan
                ? 'cta-pulse-red border-[#ef4444] bg-[#ef4444]/20 text-[#ef4444]'
                : 'cta-pulse-gold border-[#c8a86b] bg-[#c8a86b]/20 text-[#c8a86b]'
            }`}
          >
            {istBan ? 'Bannen' : 'Einloggen'}
          </motion.button>
        </div>
      )}
    </div>
  )
}
