/**
 * Das Draft-Board (/turnier/draft/:code).
 *
 * Dieselbe Seite fuer alle drei Rollen: die beiden Captains (Token im Link) und
 * Zuschauer (nur der Code). Wer kein Token hat, sieht alles, kann aber nichts
 * anklicken — das erzwingt am Ende der Server, hier wird es nur ehrlich
 * angezeigt.
 *
 * Nach dem Draft bleibt die Seite unter demselben Link abrufbar; das ist die
 * Nachbesprechung.
 */
import { useParams } from 'react-router-dom'
import { Swords, Clock, Link2, Check } from 'lucide-react'
import { useState } from 'react'
import Card from '@/components/ui/Card'
import Button from '@/components/ui/Button'
import LoadingSpinner from '@/components/ui/LoadingSpinner'
import {
  useCaptainToken,
  useCountdown,
  useDraftHeroList,
  useDraftLobby,
  useDraftLobbyAction,
} from '@/hooks/useDraftLobby'
import type { DraftHero, LobbyState } from '@/types/tournament'

function Uhr({ state }: { state: LobbyState }) {
  const rest = useCountdown(state.deadline_at)
  const laeuft = state.status === 'in_progress' && rest !== null
  const knapp = laeuft && rest !== null && rest <= 10

  if (state.status === 'completed') {
    return <div className="font-display text-2xl text-primary">Fertig</div>
  }
  if (!laeuft) {
    return <div className="font-display text-2xl text-muted">—</div>
  }
  return (
    <div className={`font-display text-4xl tabular-nums ${knapp ? 'text-danger' : 'text-foreground'}`}>
      {rest}
      <span className="ml-1 text-base text-muted">s</span>
    </div>
  )
}

function HeldKachel({
  held,
  vergeben,
  klickbar,
  onClick,
}: {
  held: DraftHero
  vergeben: boolean
  klickbar: boolean
  onClick: () => void
}) {
  return (
    <button
      type="button"
      disabled={!klickbar || vergeben}
      onClick={onClick}
      title={held.name}
      className={`group relative overflow-hidden rounded-lg border transition-all ${
        vergeben
          ? 'border-border/40 opacity-25 grayscale'
          : klickbar
            ? 'border-border hover:border-primary hover:-translate-y-0.5 cursor-pointer'
            : 'border-border/60 cursor-default'
      }`}
    >
      <img src={held.image_url} alt={held.name} loading="lazy" className="aspect-square w-full object-cover" />
      <div className="absolute inset-x-0 bottom-0 bg-gradient-to-t from-black/90 to-transparent px-1 pb-1 pt-3">
        <span className="block truncate text-[10px] font-semibold uppercase tracking-wide text-foreground">
          {held.name}
        </span>
      </div>
    </button>
  )
}

function TeamSpalte({
  name,
  picks,
  bans,
  amZug,
  reserve,
}: {
  name: string
  picks: string[]
  bans: string[]
  amZug: boolean
  reserve: number | null
}) {
  return (
    <Card className={`p-4 ${amZug ? 'border-primary' : ''}`}>
      <div className="flex items-baseline justify-between gap-2">
        <h2 className={`truncate text-lg ${amZug ? 'text-primary' : 'text-foreground'}`}>{name}</h2>
        {reserve !== null && (
          <span className="shrink-0 text-xs text-muted" title="Reserve">
            <Clock size={11} className="mr-1 inline" />
            {reserve}s
          </span>
        )}
      </div>

      <div className="mt-3 space-y-1.5">
        {picks.length === 0 && <div className="text-xs text-muted">Noch keine Picks</div>}
        {picks.map((h) => (
          <div key={h} className="rounded border border-border bg-background/40 px-2.5 py-1.5 text-sm text-foreground">
            {h}
          </div>
        ))}
      </div>

      {bans.length > 0 && (
        <div className="mt-4">
          <div className="mb-1.5 text-[10px] font-semibold uppercase tracking-wider text-muted">Gebannt</div>
          <div className="flex flex-wrap gap-1.5">
            {bans.map((h) => (
              <span key={h} className="rounded border border-danger/40 px-2 py-0.5 text-xs text-danger line-through">
                {h}
              </span>
            ))}
          </div>
        </div>
      )}
    </Card>
  )
}

export default function DraftBoard() {
  const { code } = useParams<{ code: string }>()
  const token = useCaptainToken(code)
  const lobby = useDraftLobby(code)
  const helden = useDraftHeroList()
  const aktion = useDraftLobbyAction(code)
  const [kopiert, setKopiert] = useState(false)

  if (lobby.isLoading) {
    return <div className="flex justify-center py-24"><LoadingSpinner /></div>
  }
  if (lobby.isError || !lobby.data) {
    return (
      <div className="mx-auto max-w-lg px-4 py-24 text-center">
        <h1 className="text-2xl text-foreground">Diesen Draft gibt es nicht</h1>
        <p className="mt-2 text-sm text-muted">Vielleicht ein Tippfehler im Code?</p>
      </div>
    )
  }

  const s = lobby.data
  const heldenListe = helden.data?.heroes ?? []
  const vergeben = new Set([...s.bans, ...s.picks_team1, ...s.picks_team2])
  const laeuft = s.status === 'in_progress'
  // Wer ein Token hat, darf es versuchen. Ob er wirklich dran ist, entscheidet
  // der Server — ein falscher Versuch kostet nur eine Fehlermeldung.
  const darfKlicken = laeuft && !!token
  const zugText = s.current_action_type === 'ban' ? 'bannt' : 'pickt'
  const zugTeam = s.current_team_slot === 1 ? s.team1_name : s.team2_name

  const linkKopieren = async () => {
    try {
      await navigator.clipboard.writeText(`${window.location.origin}/turnier/draft/${s.code}`)
      setKopiert(true)
      window.setTimeout(() => setKopiert(false), 1600)
    } catch {
      setKopiert(false)
    }
  }

  return (
    <div className="mx-auto max-w-6xl px-4 py-8">
      {/* Kopf: wer ist dran, wie lange noch */}
      <Card className="p-5">
        <div className="flex flex-wrap items-center justify-between gap-4">
          <div className="flex items-center gap-2 text-primary">
            <Swords size={16} />
            <span className="font-mono text-sm tracking-widest">{s.code}</span>
          </div>

          <div className="text-center">
            {s.status === 'completed' ? (
              <div className="text-sm text-muted">Draft abgeschlossen</div>
            ) : (
              <div className="text-sm text-foreground">
                <span className="font-semibold text-primary">{zugTeam}</span> {zugText}
              </div>
            )}
            <Uhr state={s} />
          </div>

          <Button variant="secondary" size="sm" onClick={linkKopieren}>
            {kopiert ? <Check size={14} /> : <Link2 size={14} />}
            <span className="ml-1.5">{kopiert ? 'Kopiert' : 'Link teilen'}</span>
          </Button>
        </div>

        {!token && s.status === 'in_progress' && (
          <p className="mt-3 text-xs text-muted">
            Du schaust zu. Zum Draften brauchst du den Captain-Link.
          </p>
        )}
        {aktion.isError && (
          <p className="mt-3 text-sm text-danger">{(aktion.error as Error).message}</p>
        )}
      </Card>

      <div className="mt-6 grid gap-6 lg:grid-cols-[220px_1fr_220px]">
        <TeamSpalte
          name={s.team1_name ?? 'Team 1'}
          picks={s.picks_team1}
          bans={s.actions.filter((a) => a.team_slot === 1 && a.action_type === 'ban' && a.hero_name).map((a) => a.hero_name!)}
          amZug={laeuft && s.current_team_slot === 1}
          reserve={s.team1_reserve_left}
        />

        <div>
          {helden.isLoading ? (
            <div className="flex justify-center py-12"><LoadingSpinner /></div>
          ) : (
            <div className="grid grid-cols-4 gap-2 sm:grid-cols-6 lg:grid-cols-8">
              {heldenListe.map((h) => (
                <HeldKachel
                  key={h.id}
                  held={h}
                  vergeben={vergeben.has(h.name)}
                  klickbar={darfKlicken}
                  onClick={() => token && aktion.mutate({ token, heroName: h.name })}
                />
              ))}
            </div>
          )}
        </div>

        <TeamSpalte
          name={s.team2_name ?? 'Team 2'}
          picks={s.picks_team2}
          bans={s.actions.filter((a) => a.team_slot === 2 && a.action_type === 'ban' && a.hero_name).map((a) => a.hero_name!)}
          amZug={laeuft && s.current_team_slot === 2}
          reserve={s.team2_reserve_left}
        />
      </div>
    </div>
  )
}
