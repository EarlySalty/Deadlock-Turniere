import { useMemo, useState } from 'react'
import { motion } from 'framer-motion'
import { ChevronDown, ChevronUp, Search } from 'lucide-react'
import type { DraftHero } from '@/types/draft'
import { heroImageUrl } from '@/hooks/draftLobbyState'
import { ROT } from './farben'

function LeistenKopf({
  modus,
  farbe,
  suchtext,
  onSuche,
  offen,
  onUmschalten,
}: {
  modus: 'ban' | 'pick'
  farbe: string
  suchtext: string
  onSuche: (text: string) => void
  offen: boolean
  onUmschalten: () => void
}) {
  return (
    <div className="flex items-center gap-2.5">
      <span
        className={`rounded px-2 py-1 text-[9px] font-black uppercase tracking-[0.2em] ${
          modus === 'ban'
            ? 'border border-[#ef444466] bg-[#ef4444]/15 text-[#ef4444]'
            : 'border bg-[#c8a86b]/10'
        }`}
        style={modus === 'pick' ? { color: farbe, borderColor: `${farbe}66` } : undefined}
      >
        {modus === 'ban' ? 'Ban' : 'Pick'}
      </span>
      <span className="relative flex-1">
        <Search
          size={11}
          className="pointer-events-none absolute left-2.5 top-1/2 -translate-y-1/2 text-white/30"
        />
        <input
          value={suchtext}
          onChange={(e) => onSuche(e.target.value)}
          placeholder="Suchen..."
          className="w-full rounded-md border border-white/[0.08] bg-white/[0.03] py-1.5 pl-7 pr-2 text-[10px] text-white placeholder:text-white/25 focus:border-white/25 focus:outline-none"
        />
      </span>
      <button
        type="button"
        onClick={onUmschalten}
        aria-label={offen ? 'Heldenleiste einklappen' : 'Heldenleiste ausklappen'}
        className="rounded-md p-1.5 text-white/40 transition-colors hover:text-white"
      >
        {offen ? <ChevronDown size={14} /> : <ChevronUp size={14} />}
      </button>
    </div>
  )
}

function HeldenRaster({
  helden,
  vergeben,
  amZug,
  auswahl,
  farbe,
  onAuswahl,
}: {
  helden: DraftHero[]
  vergeben: Set<string>
  amZug: boolean
  auswahl: string | null
  farbe: string
  onAuswahl: (name: string) => void
}) {
  return (
    <div className="draft-hero-grid-v2 pt-2">
      {helden.map((held, i) => {
        const weg = vergeben.has(held.name)
        const klickbar = amZug && !weg
        const gewaehlt = auswahl === held.name
        const portrait = heroImageUrl(held.image_url)
        return (
          <motion.button
            key={held.id}
            type="button"
            disabled={!klickbar}
            onClick={() => onAuswahl(held.name)}
            title={held.name}
            initial={{ opacity: 0, y: 10, scale: 0.98 }}
            animate={{ opacity: 1, y: 0, scale: 1 }}
            transition={{ delay: i * 0.012, duration: 0.25, ease: 'easeOut' }}
            whileHover={klickbar ? { scale: 1.25, zIndex: 20, y: -3 } : undefined}
            whileTap={klickbar ? { scale: 0.88 } : undefined}
            style={gewaehlt ? { boxShadow: `0 0 0 2px ${farbe}` } : undefined}
            className={`relative aspect-square w-[38px] overflow-hidden rounded-lg border border-white/[0.06] lg:w-[42px] xl:w-[44px] ${
              weg ? 'grayscale opacity-35' : ''
            } ${klickbar ? 'cursor-pointer' : 'cursor-default'}`}
          >
            {portrait && (
              <img src={portrait} alt={held.name} loading="lazy" className="h-full w-full object-cover" />
            )}
          </motion.button>
        )
      })}
    </div>
  )
}

export default function HeroLeiste({
  helden,
  vergeben,
  modus,
  amZug,
  auswahl,
  farbe,
  onAuswahl,
}: {
  helden: DraftHero[]
  vergeben: Set<string>
  modus: 'ban' | 'pick'
  amZug: boolean
  auswahl: string | null
  farbe: string
  onAuswahl: (name: string) => void
}) {
  const [suchtext, setSuchtext] = useState('')
  const [offenDesktop, setOffenDesktop] = useState(true)
  const [offenMobil, setOffenMobil] = useState(false)

  const gefiltert = useMemo(() => {
    const nadel = suchtext.trim().toLowerCase()
    if (!nadel) return helden
    return helden.filter((h) => h.name.toLowerCase().includes(nadel))
  }, [helden, suchtext])

  const raster = (
    <HeldenRaster
      helden={gefiltert}
      vergeben={vergeben}
      amZug={amZug}
      auswahl={auswahl}
      farbe={modus === 'ban' ? ROT : farbe}
      onAuswahl={onAuswahl}
    />
  )

  return (
    <>
      <div className="absolute bottom-4 left-1/2 z-20 hidden w-[calc(100%-310px)] max-w-[1000px] -translate-x-1/2 rounded-xl border border-white/[0.08] bg-[#101010]/95 p-3 shadow-[0_10px_40px_rgba(0,0,0,0.6)] backdrop-blur-md md:block">
        <LeistenKopf
          modus={modus}
          farbe={farbe}
          suchtext={suchtext}
          onSuche={setSuchtext}
          offen={offenDesktop}
          onUmschalten={() => setOffenDesktop((v) => !v)}
        />
        {offenDesktop && raster}
      </div>
      <div className="fixed inset-x-0 bottom-0 z-40 md:hidden">
        <motion.div
          initial={{ y: 80 }}
          animate={{ y: 0 }}
          transition={{ duration: 0.3, ease: 'easeOut' }}
          className="mx-2 mb-2 rounded-xl border border-white/[0.08] bg-[#101010]/95 p-3 shadow-[0_10px_40px_rgba(0,0,0,0.6)] backdrop-blur-md"
        >
          {offenMobil ? (
            <>
              <LeistenKopf
                modus={modus}
                farbe={farbe}
                suchtext={suchtext}
                onSuche={setSuchtext}
                offen
                onUmschalten={() => setOffenMobil(false)}
              />
              <div className="max-h-[38vh] overflow-y-auto">{raster}</div>
            </>
          ) : (
            <button
              type="button"
              onClick={() => setOffenMobil(true)}
              aria-label="Heldenleiste ausklappen"
              className="flex w-full items-center justify-center gap-2 py-1 text-[10px] font-bold uppercase tracking-[0.2em] text-white/50"
            >
              <ChevronUp size={14} />
              Helden
            </button>
          )}
        </motion.div>
      </div>
    </>
  )
}
