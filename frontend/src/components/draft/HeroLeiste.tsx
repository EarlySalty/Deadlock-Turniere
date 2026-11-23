import { useMemo, useState } from 'react'
import { Search } from 'lucide-react'
import type { DraftHero } from '@/types/draft'
import { heroImageUrl } from '@/hooks/draftLobbyState'
import { ROT } from './farben'

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
  const gefiltert = useMemo(() => {
    const nadel = suchtext.trim().toLocaleLowerCase('de')
    if (!nadel) return helden
    return helden.filter((h) => h.name.toLocaleLowerCase('de').includes(nadel))
  }, [helden, suchtext])
  const verbleibend = helden.filter((h) => !vergeben.has(h.name)).length
  const akzent = modus === 'ban' ? ROT : farbe

  return (
    <section className="draft-hero-panel" aria-label="Heldenauswahl">
      <div className="flex flex-col gap-3 border-b border-white/10 pb-4 sm:flex-row sm:items-center sm:justify-between">
        <div className="flex items-center gap-3">
          <span
            className="rounded border px-3 py-1.5 text-xs font-black uppercase tracking-[0.16em]"
            style={{ borderColor: `${akzent}88`, color: akzent, backgroundColor: `${akzent}15` }}
          >
            {modus === 'ban' ? 'Bannen' : 'Picken'}
          </span>
          <div>
            <h2 className="font-display text-xl font-black uppercase tracking-wide text-[#f1e9da]">
              Heldenauswahl
            </h2>
            <p className="text-xs text-white/55">
              {verbleibend} verfügbar · {helden.length} insgesamt
            </p>
          </div>
        </div>
        <label className="relative block w-full sm:w-56">
          <span className="sr-only">Helden suchen</span>
          <Search aria-hidden="true" size={16} className="pointer-events-none absolute left-3 top-1/2 -translate-y-1/2 text-white/50" />
          <input
            value={suchtext}
            onChange={(event) => setSuchtext(event.target.value)}
            placeholder="Helden suchen"
            autoComplete="off"
            className="w-full rounded-md border border-white/20 bg-[#0b1019] py-2.5 pl-10 pr-3 text-sm text-white placeholder:text-white/50 focus-visible:border-[#c8a86b] focus-visible:outline-none"
          />
        </label>
      </div>

      <p className="py-3 text-sm text-white/70" role="status">
        {amZug
          ? 'Du bist am Zug. Wähle einen Helden und bestätige deine Entscheidung.'
          : 'Warte auf die Entscheidung des aktiven Captains.'}
      </p>

      <div className="draft-hero-grid-v2" aria-label="Verfügbare Helden">
        {gefiltert.map((held) => {
          const weg = vergeben.has(held.name)
          const klickbar = amZug && !weg
          const gewaehlt = auswahl === held.name
          const portrait = heroImageUrl(held.image_url)
          return (
            <button
              key={held.id}
              type="button"
              disabled={!klickbar}
              onClick={() => onAuswahl(held.name)}
              aria-pressed={gewaehlt}
              aria-label={`${held.name}${weg ? ', bereits gewählt oder gebannt' : ''}`}
              title={held.name}
              className={`draft-hero-card group relative aspect-[4/5] overflow-hidden rounded-md border text-left transition-colors focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-[#c8a86b] ${weg ? 'grayscale opacity-35' : ''} ${klickbar ? 'cursor-pointer hover:brightness-125' : 'cursor-default'}`}
              style={{
                borderColor: gewaehlt ? akzent : 'rgba(217,195,155,.22)',
                boxShadow: gewaehlt ? `inset 0 0 0 2px ${akzent}` : undefined,
              }}
            >
              {portrait ? (
                <img src={portrait} alt="" loading="lazy" className="h-full w-full object-cover object-top" />
              ) : (
                <span className="flex h-full items-center justify-center bg-[#253044] font-display text-2xl font-bold text-white/40">
                  {held.name.slice(0, 2).toUpperCase()}
                </span>
              )}
              <span className="absolute inset-x-0 bottom-0 bg-gradient-to-t from-black via-black/80 to-transparent px-1.5 pb-1.5 pt-5 text-center text-[11px] font-bold leading-tight text-white sm:text-xs">
                {held.name}
              </span>
              {gewaehlt && (
                <span className="absolute inset-x-0 top-0 bg-[#c8a86b] px-1 py-1 text-center text-[10px] font-black uppercase text-[#15120e]">
                  Ausgewählt
                </span>
              )}
            </button>
          )
        })}
      </div>
      {gefiltert.length === 0 && (
        <p className="py-8 text-center text-sm text-white/60">Keine Helden gefunden.</p>
      )}
    </section>
  )
}
