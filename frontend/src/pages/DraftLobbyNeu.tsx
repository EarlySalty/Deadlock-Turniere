import { useState } from 'react'
import { useNavigate } from 'react-router-dom'
import { motion } from 'framer-motion'
import { Play, Swords } from 'lucide-react'
import DraftHintergrund from '@/components/draft/DraftHintergrund'
import { BLAU, GOLD, TINTE } from '@/components/draft/farben'
import { useDraftHeroList, useDraftRaumAnlegen } from '@/hooks/useDraftLobby'
import { heroImageUrl } from '@/hooks/draftLobbyState'

const BANS_OPTIONEN = [0, 1, 2, 3, 4, 5, 6]
const TIMER_OPTIONEN = [0, 30, 45, 60, 90]

function PillGruppe({
  label,
  optionen,
  aktiv,
  onWahl,
}: {
  label: string
  optionen: { wert: number; text: string; deaktiviert?: boolean }[]
  aktiv: number
  onWahl: (wert: number) => void
}) {
  return (
    <div>
      <div className="mb-2 text-[9px] uppercase tracking-[0.3em] text-white/30">{label}</div>
      <div className="flex flex-wrap gap-1.5">
        {optionen.map((o) => {
          const gewaehlt = o.wert === aktiv
          return (
            <button
              key={o.wert}
              type="button"
              disabled={o.deaktiviert}
              onClick={() => onWahl(o.wert)}
              className={`rounded-full border px-3 py-1 text-[10px] font-bold uppercase tracking-[0.15em] transition-colors ${
                gewaehlt
                  ? 'border-[#c8a86b] bg-[#c8a86b]/10 text-[#c8a86b]'
                  : 'border-white/10 text-white/40 hover:border-white/25 hover:text-white/70'
              } ${o.deaktiviert ? 'cursor-not-allowed opacity-25 hover:border-white/10 hover:text-white/40' : ''}`}
            >
              {o.text}
            </button>
          )
        })}
      </div>
    </div>
  )
}

function VorschauPunkte({ farbe }: { farbe: string }) {
  return (
    <div className="flex gap-1.5">
      {Array.from({ length: 6 }).map((_, i) => (
        <span key={i} className="h-1.5 w-1.5 rounded-full" style={{ backgroundColor: farbe }} />
      ))}
    </div>
  )
}

export default function DraftLobbyNeu() {
  const navigate = useNavigate()
  const helden = useDraftHeroList()
  const anlegen = useDraftRaumAnlegen()
  const [segment, setSegment] = useState<'anlegen' | 'beitreten'>('anlegen')
  const [team1, setTeam1] = useState('Team 1')
  const [team2, setTeam2] = useState('Team 2')
  const [bans, setBans] = useState(2)
  const [timer, setTimer] = useState(30)
  const [beitrittsCode, setBeitrittsCode] = useState('')

  const heldenListe = helden.data?.heroes ?? []

  const draftStarten = (e: React.FormEvent) => {
    e.preventDefault()
    anlegen.mutate(
      {
        team1_name: team1.trim() || undefined,
        team2_name: team2.trim() || undefined,
        bans_per_team: bans,
        round_seconds: timer,
      },
      { onSuccess: (raum) => navigate(`/draft/${raum.code}`) },
    )
  }

  const beitreten = (e: React.FormEvent) => {
    e.preventDefault()
    const code = beitrittsCode.trim().toUpperCase()
    if (code.length < 4) return
    navigate(`/draft/${code}`)
  }

  return (
    <div className="relative -my-8 min-h-[calc(100vh-5rem)] overflow-hidden">
      <DraftHintergrund />
      {heldenListe.length > 0 && (
        <div
          className="pointer-events-none absolute inset-0 flex flex-wrap content-start gap-1 overflow-hidden opacity-[0.08] grayscale"
          style={{
            maskImage: 'radial-gradient(ellipse 80% 60% at 50% 40%, black 20%, transparent 75%)',
            WebkitMaskImage:
              'radial-gradient(ellipse 80% 60% at 50% 40%, black 20%, transparent 75%)',
          }}
          aria-hidden="true"
        >
          {heldenListe.slice(0, 40).map((h) => {
            const bild = heroImageUrl(h.image_url)
            return bild ? (
              <img key={h.id} src={bild} alt="" className="h-24 w-24 rounded-lg object-cover" />
            ) : null
          })}
        </div>
      )}

      <div className="relative z-10 mx-auto flex max-w-2xl flex-col items-center px-4 py-10">
        <div className="fade-in-up flex items-center gap-2 text-[#c8a86b]" style={{ animationDelay: '0.05s' }}>
          <Swords size={14} />
          <span className="text-[10px] font-bold uppercase tracking-[0.35em]">Draft</span>
        </div>
        <h1
          className="fade-in-up mt-3 text-center font-display text-4xl font-black uppercase tracking-tight text-white md:text-5xl"
          style={{ animationDelay: '0.1s' }}
        >
          Deadlock <span className="text-[#c8a86b] drop-shadow-[0_0_30px_rgba(200,168,107,0.45)]">Draft</span>
        </h1>
        <div
          className="fade-in-up mt-1 font-display text-xl font-black uppercase tracking-[0.4em] text-white/70"
          style={{ animationDelay: '0.15s' }}
        >
          Tool
        </div>
        <p className="fade-in-up mt-3 text-sm text-white/40" style={{ animationDelay: '0.2s' }}>
          Scrim- und Turnier-Draft
        </p>

        <div
          className="fade-in-up mt-7 flex w-full max-w-sm rounded-xl border border-white/[0.08] bg-white/[0.02] p-1"
          style={{ animationDelay: '0.25s' }}
        >
          {(
            [
              ['anlegen', 'Draft anlegen'],
              ['beitreten', 'Raum beitreten'],
            ] as const
          ).map(([wert, text]) => (
            <button
              key={wert}
              type="button"
              onClick={() => setSegment(wert)}
              className={`flex-1 rounded-lg px-4 py-2 text-[10px] font-bold uppercase tracking-[0.15em] transition-colors ${
                segment === wert
                  ? 'border border-[#c8a86b]/50 bg-[#c8a86b]/10 text-[#c8a86b]'
                  : 'text-white/40 hover:text-white/70'
              }`}
            >
              {text}
            </button>
          ))}
        </div>

        {segment === 'beitreten' ? (
          <form
            onSubmit={beitreten}
            className="fade-in-up mt-6 w-full max-w-sm rounded-2xl border border-white/[0.08] bg-white/[0.02] p-6 backdrop-blur-sm"
          >
            <label className="block">
              <span className="mb-2 block text-[9px] uppercase tracking-[0.3em] text-white/30">
                Raum-Code
              </span>
              <input
                value={beitrittsCode}
                onChange={(e) => setBeitrittsCode(e.target.value.toUpperCase())}
                maxLength={6}
                placeholder="ABC123"
                className="w-full rounded-lg border border-white/10 bg-black/40 px-3 py-2.5 text-center font-mono text-xl tracking-[0.3em] text-white placeholder:text-white/20 focus:border-[#c8a86b]/60 focus:outline-none"
              />
            </label>
            <button
              type="submit"
              className="mt-4 w-full rounded-lg border border-[#c8a86b] bg-[#c8a86b]/10 py-2.5 text-[11px] font-bold uppercase tracking-[0.2em] text-[#c8a86b] transition-colors hover:bg-[#c8a86b]/20"
            >
              Raum beitreten
            </button>
            <p className="mt-3 text-center text-xs text-white/30">
              Den Code hat dir dein Gegenüber geschickt.
            </p>
          </form>
        ) : (
          <form
            onSubmit={draftStarten}
            className="fade-in-up mt-6 w-full rounded-2xl border border-white/[0.08] bg-white/[0.02] p-6 backdrop-blur-sm"
            style={{ animationDelay: '0.3s' }}
          >
            <div className="grid grid-cols-[1fr_auto_1fr] items-center gap-3">
              <input
                value={team1}
                maxLength={40}
                onChange={(e) => setTeam1(e.target.value)}
                placeholder="Name..."
                className="w-full rounded-lg border border-[#c8a86b]/40 bg-black/40 px-3 py-2 text-sm font-bold uppercase tracking-wide text-[#c8a86b] placeholder:text-white/25 focus:border-[#c8a86b] focus:outline-none"
              />
              <div className="flex flex-col items-center gap-1">
                <span
                  className="flex h-12 w-12 items-center justify-center rounded-full border-2 font-display text-xs font-black"
                  style={{ borderColor: GOLD, color: GOLD }}
                >
                  6v6
                </span>
                <span className="text-[9px] uppercase tracking-[0.2em] text-white/30">
                  {timer > 0 ? `${timer}s` : 'Aus'}
                </span>
              </div>
              <input
                value={team2}
                maxLength={40}
                onChange={(e) => setTeam2(e.target.value)}
                placeholder="Name..."
                className="w-full rounded-lg border border-[#3b82f6]/40 bg-black/40 px-3 py-2 text-right text-sm font-bold uppercase tracking-wide text-[#3b82f6] placeholder:text-white/25 focus:border-[#3b82f6] focus:outline-none"
              />
            </div>

            <div className="mt-5">
              <div className="flex items-center justify-center gap-2 text-[9px] uppercase tracking-[0.3em] text-white/30">
                Live-Vorschau
                <span className="flex items-center gap-1 text-[#10b981]">
                  <span className="h-1.5 w-1.5 rounded-full bg-[#10b981]" />
                  Demo
                </span>
              </div>
              <div className="mt-2 flex items-center justify-center gap-4">
                <div className="flex flex-col items-center gap-1.5">
                  <div className="flex gap-1">
                    {Array.from({ length: bans }).map((_, i) => (
                      <span key={i} className="h-6 w-4 rounded border" style={{ borderColor: `${GOLD}55` }} />
                    ))}
                  </div>
                  <VorschauPunkte farbe={GOLD} />
                </div>
                <span className="text-[9px] font-bold uppercase tracking-[0.2em] text-white/20">
                  vs
                </span>
                <div className="flex flex-col items-center gap-1.5">
                  <div className="flex gap-1">
                    {Array.from({ length: bans }).map((_, i) => (
                      <span key={i} className="h-6 w-4 rounded border" style={{ borderColor: `${BLAU}55` }} />
                    ))}
                  </div>
                  <VorschauPunkte farbe={BLAU} />
                </div>
              </div>
            </div>

            <div className="mt-6 grid gap-4 sm:grid-cols-3">
              <PillGruppe
                label="Format"
                aktiv={0}
                onWahl={() => {}}
                optionen={[
                  { wert: 0, text: '6v6' },
                  { wert: 1, text: '4v4', deaktiviert: true },
                  { wert: 2, text: '2v2', deaktiviert: true },
                ]}
              />
              <PillGruppe
                label="Bans"
                aktiv={bans}
                onWahl={setBans}
                optionen={BANS_OPTIONEN.map((wert) => ({
                  wert,
                  text: wert === 0 ? '-' : String(wert),
                }))}
              />
              <PillGruppe
                label="Timer"
                aktiv={timer}
                onWahl={setTimer}
                optionen={TIMER_OPTIONEN.map((wert) => ({
                  wert,
                  text: wert === 0 ? 'Aus' : `${wert}s`,
                }))}
              />
            </div>

            {anlegen.isError && (
              <p className="mt-4 text-center text-xs text-[#ef4444]">
                Der Draft antwortet gerade nicht, bitte nochmal versuchen.
              </p>
            )}

            <motion.button
              type="submit"
              whileTap={{ scale: 0.98 }}
              disabled={anlegen.isPending}
              className="cta-pulse-gold mt-6 flex w-full items-center justify-center gap-2 rounded-lg py-3.5 text-sm font-black uppercase tracking-[0.25em] transition-opacity disabled:opacity-50"
              style={{ backgroundColor: GOLD, color: TINTE }}
            >
              <Play size={15} />
              {anlegen.isPending ? 'Wird angelegt...' : 'Draft starten'}
            </motion.button>
            <p className="mt-3 text-center text-[11px] text-white/30">
              Auf der nächsten Seite bekommst du einen Raum-Code zum Teilen.
            </p>
          </form>
        )}
      </div>
    </div>
  )
}
