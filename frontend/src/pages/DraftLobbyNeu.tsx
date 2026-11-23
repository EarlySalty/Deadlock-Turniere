import { useState } from 'react'
import { useNavigate } from 'react-router-dom'
import { motion } from 'framer-motion'
import { Play, Swords } from 'lucide-react'
import DraftHintergrund from '@/components/draft/DraftHintergrund'
import TeamModeNav from '@/components/draft/TeamModeNav'
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
      <div className="mb-2 text-xs font-semibold uppercase tracking-[0.16em] text-white/65">{label}</div>
      <div className="flex flex-wrap gap-1.5">
        {optionen.map((o) => {
          const gewaehlt = o.wert === aktiv
          return (
            <button
              key={o.wert}
              type="button"
              disabled={o.deaktiviert}
              onClick={() => onWahl(o.wert)}
              className={`min-h-10 rounded-md border px-3 py-2 text-sm font-bold uppercase tracking-wide transition-colors focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-[#c8a86b] ${
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

      <div className="relative z-10 mx-auto flex max-w-4xl flex-col items-center px-4 py-12 md:py-16">
        <TeamModeNav />
        <div className="fade-in-up flex items-center gap-2 text-[#d9bd86]" style={{ animationDelay: '0.05s' }}>
          <Swords size={14} />
          <span className="text-[10px] font-bold uppercase tracking-[0.35em]">Draft</span>
        </div>
        <h1
          className="ddc-occult-title fade-in-up mt-3 text-center text-5xl font-black uppercase tracking-tight text-white md:text-7xl"
          style={{ animationDelay: '0.1s' }}
        >
          Deadlock <span className="text-[#c8a86b] drop-shadow-[0_0_30px_rgba(200,168,107,0.45)]">Draft</span>
        </h1>
        <div
          className="fade-in-up mt-2 font-display text-lg font-bold uppercase tracking-[0.22em] text-white/70 md:text-xl"
          style={{ animationDelay: '0.15s' }}
        >
          Scrim · Custom Lobby
        </div>
        <p className="fade-in-up mt-3 text-base text-white/70" style={{ animationDelay: '0.2s' }}>
          Captains wählen ihre Helden. Keine Registrierung für freie Draft-Räume nötig.
        </p>

        <div
          className="fade-in-up mt-8 flex w-full max-w-lg rounded-md border border-[#c8a86b]/25 bg-[#141922]/95 p-1.5"
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
              className={`min-h-11 flex-1 rounded-md px-4 py-2 text-sm font-bold uppercase tracking-wide transition-colors ${
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
            className="fade-in-up mt-6 w-full max-w-lg rounded-lg border border-[#c8a86b]/25 bg-[#141922]/95 p-7 shadow-[0_20px_60px_rgba(0,0,0,0.5)]"
          >
            <label className="block">
              <span className="mb-2 block text-[9px] uppercase tracking-[0.3em] text-white/30">
                Raum-Code
              </span>
              <input
                value={beitrittsCode}
                onChange={(e) => setBeitrittsCode(e.target.value.toUpperCase())}
                maxLength={8}
                placeholder="ABCD2345"
                autoCapitalize="characters"
                spellCheck={false}
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
            className="fade-in-up mt-6 w-full rounded-lg border border-[#c8a86b]/25 bg-[#141922]/95 p-6 shadow-[0_20px_60px_rgba(0,0,0,0.5)] md:p-10"
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
                Draft-Besetzung · Vorschau
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
              <div>
                <div className="mb-2 text-xs font-semibold uppercase tracking-[0.16em] text-white/65">Format</div>
                <div className="flex min-h-10 items-center rounded-md border border-[#c8a86b]/50 bg-[#c8a86b]/10 px-4 text-sm font-bold text-[#d9bd86]">
                  6 gegen 6
                </div>
              </div>
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
              {anlegen.isPending ? 'Wird angelegt...' : 'Draft-Raum erstellen'}
            </motion.button>
            <p className="mt-3 text-center text-[11px] text-white/30">
              Anschließend erhalten beide Captains einen Raum-Code und können ihren Platz einnehmen.
            </p>
          </form>
        )}
      </div>
    </div>
  )
}
