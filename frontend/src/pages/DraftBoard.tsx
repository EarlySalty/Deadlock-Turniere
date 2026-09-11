import { useEffect, useMemo, useRef, useState } from 'react'
import { Link, useNavigate, useParams } from 'react-router-dom'
import { AnimatePresence, motion } from 'framer-motion'
import { Eye } from 'lucide-react'
import LoadingSpinner from '@/components/ui/LoadingSpinner'
import DraftHintergrund from '@/components/draft/DraftHintergrund'
import RaumCode from '@/components/draft/RaumCode'
import TeamKarte from '@/components/draft/TeamKarte'
import BanLeiste from '@/components/draft/BanLeiste'
import TeamSpalte from '@/components/draft/TeamSpalte'
import HeroLeiste from '@/components/draft/HeroLeiste'
import SplashBuehne from '@/components/draft/SplashBuehne'
import BanStempel from '@/components/draft/BanStempel'
import PickAnsage from '@/components/draft/PickAnsage'
import Konfetti from '@/components/draft/Konfetti'
import Endkarten from '@/components/draft/Endkarten'
import LobbyBox from '@/components/draft/LobbyBox'
import KopierKnopf from '@/components/draft/KopierKnopf'
import RotesX from '@/components/draft/RotesX'
import { BLAU, GOLD, teamFarbe } from '@/components/draft/farben'
import {
  DraftApiFehler,
  useClaimCaptain,
  useCountdown,
  useCountdownText,
  useDraftAktion,
  useDraftHeroList,
  useLeaveCaptain,
  useLobbyRetry,
  usePhasenKopf,
  useReadyTeam,
  useRematch,
  useScrimLobby,
} from '@/hooks/useDraftLobby'
import { heroCardImageUrl, heroImageUrl, loescheClaimToken, raumUrl } from '@/hooks/draftLobbyState'
import type { DraftHero, DraftRaumZustand } from '@/types/draft'

interface Uebergang {
  art: 'ban' | 'pick'
  held: DraftHero
  team: 1 | 2
  teamName: string
}

function Kopf({ zustand }: { zustand: DraftRaumZustand }) {
  const rest = useCountdown(zustand.deadline_at)
  const text = useCountdownText(rest)
  const zug = usePhasenKopf(zustand.sequence, zustand.current_action_index)
  const rot = rest !== null && rest <= 5
  const abgelaufen = rest !== null && rest === 0
  const teamName = zug ? (zug.team === 1 ? zustand.team1.name : zustand.team2.name) : ''
  const farbe = zug ? teamFarbe(zug.team) : undefined
  const amZug = zug !== null && zustand.you.team === zug.team

  return (
    <div
      key={abgelaufen ? 'abgelaufen' : 'laeuft'}
      className={`relative z-20 flex flex-col items-center ${abgelaufen ? 'draft-shake' : ''}`}
    >
      <AnimatePresence mode="wait">
        <motion.div
          key={zustand.current_action_index}
          initial={{ y: 10, opacity: 0 }}
          animate={{ y: 0, opacity: 1 }}
          exit={{ y: 5, opacity: 0 }}
          transition={{ duration: 0.2 }}
          className="flex flex-col items-center"
        >
          <div className="text-[10px] uppercase tracking-[0.3em] text-white/50">
            {zug?.label ?? ''}{' '}
            <span className="text-white/25">{zug?.anteil ?? ''}</span>
          </div>
          <div className="mt-1 text-sm font-bold uppercase tracking-wide" style={{ color: farbe }}>
            {zug ? `${teamName} · ${zug.action === 'ban' ? 'Bannen' : 'Pickt'}` : ''}
          </div>
        </motion.div>
      </AnimatePresence>
      {amZug && (
        <motion.div
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
          className="mt-1 text-[10px] font-bold uppercase tracking-[0.2em] text-[#10b981]"
        >
          Du bist dran
        </motion.div>
      )}
      {rest !== null && (
        <div
          className={`mt-1 font-mono text-lg tabular-nums ${rot ? 'text-[#ef4444]' : 'text-white/80'}`}
        >
          <span key={rest} className={`inline-block ${rot ? 'draft-shake' : ''}`}>
            {text}
          </span>
        </div>
      )}
    </div>
  )
}

function Warteraum({
  zustand,
  claimLaeuft,
  claimFehler,
  aktionsFehler,
  onClaim,
  onReady,
  onLeave,
}: {
  zustand: DraftRaumZustand
  claimLaeuft: boolean
  claimFehler: string | null
  aktionsFehler: string | null
  onClaim: (team: 1 | 2) => void
  onReady: () => void
  onLeave: () => void
}) {
  const beideBereit = zustand.team1.ready && zustand.team2.ready

  return (
    <div className="relative min-h-[calc(100vh-5rem)] overflow-hidden">
      <DraftHintergrund />
      <div className="relative z-10 mx-auto flex min-h-[calc(100vh-5rem)] max-w-3xl flex-col items-center justify-center gap-6 px-4 py-10">
        <div className="fade-in-up flex items-center gap-2 rounded-full border border-white/[0.08] bg-white/[0.03] px-4 py-1.5">
          <span className="h-1.5 w-1.5 animate-pulse rounded-full bg-[#10b981]" />
          <span className="text-[10px] uppercase tracking-[0.25em] text-white/50">
            Warten auf Spieler
          </span>
        </div>

        <motion.h1
          initial={{ y: 20, opacity: 0 }}
          animate={{ y: 0, opacity: 1 }}
          transition={{ duration: 0.4, ease: 'easeOut' }}
          className="text-center font-display text-4xl font-black uppercase tracking-tight md:text-6xl"
        >
          <span style={{ color: GOLD }}>{zustand.team1.name}</span>{' '}
          <span className="align-middle text-sm font-bold text-white/30">vs</span>{' '}
          <span style={{ color: BLAU }}>{zustand.team2.name}</span>
        </motion.h1>

        <RaumCode code={zustand.code} />

        <div className="text-[9px] uppercase tracking-[0.3em] text-white/25">
          6v6 · {zustand.bans_per_team} Bans ·{' '}
          {zustand.round_seconds > 0 ? `${zustand.round_seconds}s Timer` : 'ohne Timer'}
        </div>

        <div className="flex w-full flex-col items-center justify-center gap-4 md:flex-row md:gap-8">
          {([1, 2] as const).map((team) => (
            <div key={team} className="flex items-center gap-8">
              {team === 2 && <div className="hidden h-28 w-px bg-white/10 md:block" />}
              <TeamKarte
                team={team}
                name={team === 1 ? zustand.team1.name : zustand.team2.name}
                claimed={team === 1 ? zustand.team1.claimed : zustand.team2.claimed}
                ready={team === 1 ? zustand.team1.ready : zustand.team2.ready}
                duBistEs={zustand.you.team === team}
                gesperrt={claimLaeuft}
                onClaim={() => onClaim(team as 1 | 2)}
                onReady={onReady}
                onLeave={onLeave}
              />
            </div>
          ))}
        </div>

        {claimFehler && <p className="text-xs text-[#ef4444]">{claimFehler}</p>}
        {aktionsFehler && <p className="text-xs text-[#ef4444]">{aktionsFehler}</p>}

        {beideBereit && (
          <motion.div
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            className="flex items-center gap-3"
          >
            <span className="h-10 w-10 animate-spin rounded-full border-2 border-[#10b981]/30 border-t-[#10b981]" />
            <span className="text-xs uppercase tracking-[0.25em] text-[#10b981]">
              Draft startet...
            </span>
          </motion.div>
        )}

        <div className="flex flex-col items-center gap-3">
          <div className="flex items-center gap-2 text-xs text-white/30">
            <Eye size={13} />
            {zustand.spectators === 0
              ? 'Keine Zuschauer'
              : zustand.spectators === 1
                ? '1 schaut zu'
                : `${zustand.spectators} schauen zu`}
          </div>
          <motion.p
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            transition={{ delay: 0.7 }}
            className="text-[11px] text-white/25"
          >
            Teile den Raum-Link und übernimm deinen Captain-Platz. Beide Captains müssen Bereit
            drücken.
          </motion.p>
        </div>

        <Link
          to="/draft"
          className="mt-2 text-[9px] uppercase tracking-[0.3em] text-white/20 transition-colors hover:text-white/50"
        >
          Neuen Draft anlegen
        </Link>
      </div>
    </div>
  )
}

function DraftTisch({
  zustand,
  helden,
  vergeben,
  auswahl,
  onAuswahl,
  onBestaetigen,
  bestaetigtLaeuft,
  aktionsFehler,
  uebergang,
}: {
  zustand: DraftRaumZustand
  helden: DraftHero[]
  vergeben: Set<string>
  auswahl: string | null
  onAuswahl: (name: string | null) => void
  onBestaetigen: () => void
  bestaetigtLaeuft: boolean
  aktionsFehler: string | null
  uebergang: Uebergang | null
}) {
  const zug = usePhasenKopf(zustand.sequence, zustand.current_action_index)
  const amZug = zug !== null && zustand.you.team === zug.team
  const istBanZug = zug?.action === 'ban'
  const aktivesTeam = zug?.team ?? null
  const farbe = zug ? teamFarbe(zug.team) : '#ffffff'

  const banne = zustand.actions.filter((a) => a.action_type === 'ban')
  const picks1 = zustand.actions.filter((a) => a.action_type === 'pick' && a.team_slot === 1)
  const picks2 = zustand.actions.filter((a) => a.action_type === 'pick' && a.team_slot === 2)

  const portraits = useMemo(() => {
    const karte = new Map<string, string>()
    for (const h of helden) {
      const url = heroImageUrl(h.image_url)
      if (url) karte.set(h.name, url)
    }
    return karte
  }, [helden])

  const ausgewaehlt = helden.find((h) => h.name === auswahl) ?? null
  const aktiverSlot1 = !istBanZug && aktivesTeam === 1 ? picks1.length : null
  const aktiverSlot2 = !istBanZug && aktivesTeam === 2 ? picks2.length : null

  return (
    <div className="relative min-h-[calc(100vh-5rem)] overflow-hidden">
      <DraftHintergrund mitte />
      <div className="pointer-events-none absolute inset-0 flex items-center justify-center">
        <span className="select-none text-center font-display text-4xl font-black uppercase tracking-tight text-white/[0.05] md:text-6xl">
          {zustand.team1.name}
          <span className="mx-3 text-[0.45em] align-middle">vs</span>
          {zustand.team2.name}
        </span>
      </div>

      <span className="absolute left-5 top-5 z-20 text-[11px] font-black uppercase tracking-[0.25em]" style={{ color: GOLD }}>
        {zustand.team1.name}
      </span>
      <span className="absolute right-5 top-5 z-20 text-[11px] font-black uppercase tracking-[0.25em]" style={{ color: BLAU }}>
        {zustand.team2.name}
      </span>

      <div className="relative z-20 flex flex-col items-center pt-4">
        <Kopf zustand={zustand} />
        <div className="mt-3">
          <BanLeiste
            bansProTeam={zustand.bans_per_team}
            banne={banne}
            aktivesTeam={aktivesTeam}
            istBanZug={!!istBanZug}
            portraits={portraits}
          />
        </div>
        {aktionsFehler && (
          <p className="mt-2 text-xs text-[#ef4444]">{aktionsFehler}</p>
        )}
      </div>

      {ausgewaehlt && zug && !uebergang && (
        <SplashBuehne
          held={ausgewaehlt}
          team={zug.team}
          teamName={zug.team === 1 ? zustand.team1.name : zustand.team2.name}
          istBan={istBanZug}
          kannBestaetigen={amZug}
          beschaeftigt={bestaetigtLaeuft}
          onBestaetigen={onBestaetigen}
        />
      )}

      <div className="relative z-10 mt-6 grid grid-cols-1 gap-4 px-4 pb-56 md:px-6 lg:grid-cols-[220px_1fr_220px] lg:pb-32">
        <TeamSpalte team={1} name={zustand.team1.name} picks={picks1} aktiverSlot={aktiverSlot1} portraits={portraits} />
        <div className="hidden lg:block" />
        <TeamSpalte team={2} name={zustand.team2.name} picks={picks2} aktiverSlot={aktiverSlot2} portraits={portraits} />
      </div>

      <HeroLeiste
        helden={helden}
        vergeben={vergeben}
        modus={istBanZug ? 'ban' : 'pick'}
        amZug={amZug}
        auswahl={auswahl}
        farbe={farbe}
        onAuswahl={(name) => onAuswahl(auswahl === name ? null : name)}
      />

      <AnimatePresence>
        {uebergang && zug && (
          <>
            {uebergang.art === 'ban' ? (
              <BanStempel key="ban-stempel" held={uebergang.held} />
            ) : (
              <PickAnsage key="pick-ansage" held={uebergang.held} team={uebergang.team} teamName={uebergang.teamName} />
            )}
          </>
        )}
      </AnimatePresence>
    </div>
  )
}

function Endbild({
  zustand,
  splashQuellen,
  onRematch,
  rematchLaeuft,
  rematchFehler,
  onRetry,
  retryLaeuft,
}: {
  zustand: DraftRaumZustand
  splashQuellen: Map<string, string>
  onRematch: () => void
  rematchLaeuft: boolean
  rematchFehler: string | null
  onRetry: () => void
  retryLaeuft: boolean
}) {
  const banne = zustand.actions.filter((a) => a.action_type === 'ban')
  const picks1 = zustand.actions.filter((a) => a.action_type === 'pick' && a.team_slot === 1)
  const picks2 = zustand.actions.filter((a) => a.action_type === 'pick' && a.team_slot === 2)

  return (
    <div className="relative min-h-[calc(100vh-5rem)] overflow-hidden">
      <DraftHintergrund />
      <Konfetti />
      <div className="relative z-10 mx-auto flex max-w-6xl flex-col items-center gap-6 px-4 py-10">
        <motion.div
          initial={{ letterSpacing: '1em', opacity: 0 }}
          animate={{ letterSpacing: '0.6em', opacity: 1 }}
          transition={{ delay: 0.1, duration: 0.8 }}
          className="text-[10px] uppercase text-white/40"
        >
          Draft abgeschlossen
        </motion.div>

        <h1 className="text-center font-display text-4xl font-black uppercase tracking-tight md:text-5xl">
          <motion.span
            initial={{ x: -30, opacity: 0 }}
            animate={{ x: 0, opacity: 1 }}
            transition={{ delay: 0.15, duration: 0.5 }}
            style={{ color: GOLD }}
          >
            {zustand.team1.name}
          </motion.span>
          <span className="mx-3 text-lg text-white/30">vs</span>
          <motion.span
            initial={{ x: 30, opacity: 0 }}
            animate={{ x: 0, opacity: 1 }}
            transition={{ delay: 0.15, duration: 0.5 }}
            style={{ color: BLAU }}
          >
            {zustand.team2.name}
          </motion.span>
        </h1>

        <div className="fade-in-up flex items-center gap-6" style={{ animationDelay: '0.18s' }}>
          {banne.length === 0 ? (
            <span className="text-[10px] uppercase tracking-[0.25em] text-white/25">
              Keine Bans
            </span>
          ) : (
            banne.map((b) => {
              const bild = splashQuellen.get(b.hero_name)
              return (
                <div key={b.sequence_index} className="relative h-9 w-9 overflow-hidden rounded border border-white/10">
                  {bild && (
                    <img src={bild} alt={b.hero_name} className="h-full w-full object-cover opacity-35 grayscale" />
                  )}
                  <RotesX groesse={16} />
                </div>
              )
            })
          )}
        </div>

        <LobbyBox lobby={zustand.lobby} onRetry={onRetry} retryLaeuft={retryLaeuft} />

        <div className="flex w-full flex-col items-center gap-8 lg:flex-row lg:items-start lg:justify-center">
          <Endkarten team={1} name={zustand.team1.name} picks={picks1} splashQuellen={splashQuellen} />
          <Endkarten team={2} name={zustand.team2.name} picks={picks2} splashQuellen={splashQuellen} />
        </div>

        <div className="fade-in-up flex items-center gap-3" style={{ animationDelay: '0.75s' }}>
          <KopierKnopf text={raumUrl(zustand.code)} label="Teilen" />
          <motion.button
            whileHover={{ scale: 1.03 }}
            whileTap={{ scale: 0.95 }}
            type="button"
            onClick={onRematch}
            disabled={rematchLaeuft}
            className="rounded-lg bg-[#c8a86b] px-5 py-2 text-[10px] font-black uppercase tracking-[0.2em] text-[#0b0b0b] disabled:opacity-50"
          >
            Rematch
          </motion.button>
          <Link
            to="/draft"
            className="text-[10px] uppercase tracking-[0.2em] text-white/40 transition-colors hover:text-white/80"
          >
            Zurück
          </Link>
        </div>

        {rematchFehler && <p className="text-xs text-[#ef4444]">{rematchFehler}</p>}
      </div>
    </div>
  )
}

export default function DraftBoard() {
  const { code } = useParams<{ code: string }>()
  const navigate = useNavigate()
  const lobby = useScrimLobby(code)
  const helden = useDraftHeroList()
  const claim = useClaimCaptain(code)
  const bereit = useReadyTeam(code)
  const verlassen = useLeaveCaptain(code)
  const aktion = useDraftAktion(code)
  const rematch = useRematch(code)
  const wiederholung = useLobbyRetry(code)
  const [auswahl, setAuswahl] = useState<string | null>(null)
  const [uebergang, setUebergang] = useState<Uebergang | null>(null)
  const bekanntRef = useRef<number | null>(null)

  const heldenListe = useMemo(() => helden.data?.heroes ?? [], [helden.data])

  const splashQuellen = useMemo(() => {
    const karte = new Map<string, string>()
    for (const h of heldenListe) {
      const url = heroCardImageUrl(h.card_image_url, h.image_url)
      if (url) karte.set(h.name, url)
    }
    return karte
  }, [heldenListe])

  const vergeben = useMemo(
    () => new Set((lobby.data?.actions ?? []).map((a) => a.hero_name)),
    [lobby.data],
  )

  useEffect(() => {
    const s = lobby.data
    if (!s) return
    if (bekanntRef.current === null) {
      bekanntRef.current = s.actions.length
      return
    }
    if (s.actions.length > bekanntRef.current) {
      const neu = s.actions[s.actions.length - 1]
      bekanntRef.current = s.actions.length
      const held = heldenListe.find((h) => h.name === neu.hero_name)
      if (!held || s.phase !== 'laeuft') return
      const id = window.setTimeout(() => {
        setAuswahl(null)
        setUebergang({
          art: neu.action_type,
          held,
          team: neu.team_slot,
          teamName: neu.team_slot === 1 ? s.team1.name : s.team2.name,
        })
      }, 0)
      return () => window.clearTimeout(id)
    }
  }, [lobby.data, heldenListe])

  useEffect(() => {
    if (!uebergang) return
    const id = window.setTimeout(
      () => setUebergang(null),
      uebergang.art === 'ban' ? 2000 : 1300,
    )
    return () => window.clearTimeout(id)
  }, [uebergang])

  if (lobby.isLoading) {
    return (
      <div className="flex min-h-[50vh] items-center justify-center">
        <LoadingSpinner />
      </div>
    )
  }

  if (lobby.isError || !lobby.data) {
    const meldung =
      lobby.error instanceof DraftApiFehler && lobby.error.status === 404
        ? 'Diesen Draft gibt es nicht. Vielleicht ein Tippfehler im Code?'
        : 'Der Draft antwortet gerade nicht, bitte die Seite neu laden.'
    return (
      <div className="mx-auto max-w-lg px-4 py-24 text-center">
        <h1 className="text-2xl text-foreground">Raum nicht gefunden</h1>
        <p className="mt-2 text-sm text-muted">{meldung}</p>
        <Link to="/draft" className="mt-6 inline-block text-xs text-primary hover:underline">
          Neuen Draft anlegen
        </Link>
      </div>
    )
  }

  const zustand = lobby.data
  const claimFehler =
    claim.isError && claim.error instanceof Error ? claim.error.message : null
  const aktionsFehler =
    bereit.isError && bereit.error instanceof Error
      ? bereit.error.message
      : verlassen.isError && verlassen.error instanceof Error
        ? verlassen.error.message
        : aktion.isError && aktion.error instanceof Error
          ? aktion.error.message
          : null

  const bestaetigen = () => {
    if (!auswahl || !code) return
    aktion.mutate(auswahl, {
      onSuccess: () => setAuswahl(null),
    })
  }

  const rematchStarten = () => {
    if (!code) return
    rematch.mutate(undefined, {
      onSuccess: (raum) => {
        setAuswahl(null)
        bekanntRef.current = null
        navigate(`/draft/${raum.code}`)
      },
    })
  }

  return (
    <div className="relative">
      <AnimatePresence mode="wait">
        {zustand.phase === 'warteraum' && (
          <motion.div
            key="warteraum"
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0, scale: 0.9 }}
            transition={{ duration: 0.3, ease: 'easeOut' }}
          >
            <Warteraum
              zustand={zustand}
              claimLaeuft={claim.isPending}
              claimFehler={claimFehler}
              aktionsFehler={aktionsFehler}
              onClaim={(team) => claim.mutate(team)}
              onReady={() => bereit.mutate({})}
              onLeave={() =>
                verlassen.mutate(
                  {},
                  {
                    onSuccess: () => {
                      if (code) loescheClaimToken(code)
                      setAuswahl(null)
                    },
                  },
                )
              }
            />
          </motion.div>
        )}
        {zustand.phase === 'laeuft' && (
          <motion.div
            key="laeuft"
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0, scale: 0.95 }}
            transition={{ duration: 0.3, ease: 'easeOut' }}
          >
            <DraftTisch
              zustand={zustand}
              helden={heldenListe}
              vergeben={vergeben}
              auswahl={auswahl}
              onAuswahl={setAuswahl}
              onBestaetigen={bestaetigen}
              bestaetigtLaeuft={aktion.isPending}
              aktionsFehler={aktionsFehler}
              uebergang={uebergang}
            />
          </motion.div>
        )}
        {zustand.phase === 'abgeschlossen' && (
          <motion.div
            key="abgeschlossen"
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0 }}
            transition={{ duration: 0.3, ease: 'easeOut' }}
          >
            <Endbild
              zustand={zustand}
              splashQuellen={splashQuellen}
              onRematch={rematchStarten}
              rematchLaeuft={rematch.isPending}
              rematchFehler={
                rematch.isError && rematch.error instanceof Error ? rematch.error.message : null
              }
              onRetry={() => wiederholung.mutate({})}
              retryLaeuft={wiederholung.isPending}
            />
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  )
}
