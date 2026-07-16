/**
 * Freie Draft-Lobby anlegen (/turnier/draft).
 *
 * Kein Login. Wer die Lobby aufmacht, bekommt drei Links: einen je Captain und
 * einen zum Zuschauen. Die Captain-Tokens liefert der Server GENAU EINMAL, beim
 * Anlegen — danach sind sie nirgends mehr abrufbar. Deshalb bleiben sie hier
 * stehen, bis der Ersteller sie verteilt hat.
 */
import { useState } from 'react'
import { Link } from 'react-router-dom'
import { Swords, Copy, Check, Eye } from 'lucide-react'
import Card from '@/components/ui/Card'
import Button from '@/components/ui/Button'
import { useCreateDraftLobby } from '@/hooks/useDraftLobby'
import type { DraftPreset, LobbyCredentials } from '@/types/tournament'

const PRESETS: { id: DraftPreset; titel: string; erklaerung: string }[] = [
  { id: 'competitive_2ban', titel: 'Competitive', erklaerung: '2 Bans pro Team, dann 6 Picks' },
  { id: 'competitive_1ban', titel: 'Ein Ban', erklaerung: '1 Ban pro Team, dann 6 Picks' },
  { id: 'quick_no_ban', titel: 'Schnell', erklaerung: 'Keine Bans, direkt picken' },
]

function LinkZeile({ beschriftung, url, betont }: { beschriftung: string; url: string; betont?: boolean }) {
  const [kopiert, setKopiert] = useState(false)
  const kopieren = async () => {
    try {
      await navigator.clipboard.writeText(url)
      setKopiert(true)
      window.setTimeout(() => setKopiert(false), 1600)
    } catch {
      setKopiert(false)
    }
  }
  return (
    <div className="flex items-center gap-3">
      <div className="min-w-0 flex-1">
        <div className={`text-xs font-semibold ${betont ? 'text-primary' : 'text-muted'}`}>{beschriftung}</div>
        <div className="truncate font-mono text-xs text-foreground/70">{url}</div>
      </div>
      <Button variant={betont ? 'primary' : 'secondary'} size="sm" onClick={kopieren}>
        {kopiert ? <Check size={14} /> : <Copy size={14} />}
        <span className="ml-1.5">{kopiert ? 'Kopiert' : 'Kopieren'}</span>
      </Button>
    </div>
  )
}

function Fertig({ zugang }: { zugang: LobbyCredentials }) {
  const basis = `${window.location.origin}/turnier/draft/${zugang.code}`
  return (
    <Card className="p-6">
      <h2 className="text-xl text-foreground">Lobby steht</h2>
      <p className="mt-1 text-sm text-muted">
        Schick jedem Captain seinen Link. Wer den Zuschauer-Link hat, sieht alles mit, kann aber nicht
        eingreifen.
      </p>

      <div className="mt-5 rounded-lg border border-border bg-background/40 p-4">
        <div className="text-xs text-muted">Draft-Code</div>
        <div className="font-display text-3xl tracking-widest text-primary">{zugang.code}</div>
      </div>

      <div className="mt-5 space-y-4">
        <LinkZeile beschriftung="Captain Team 1" url={`${basis}?t=${zugang.team1_token}`} betont />
        <LinkZeile beschriftung="Captain Team 2" url={`${basis}?t=${zugang.team2_token}`} betont />
        <LinkZeile beschriftung="Zuschauen" url={basis} />
      </div>

      <p className="mt-5 text-xs text-warning">
        Die beiden Captain-Links gibt es nur jetzt. Sobald du diese Seite verlässt, sind sie weg und die
        Lobby muss neu aufgemacht werden.
      </p>

      <div className="mt-5">
        <Link to={`/draft/${zugang.code}`}>
          <Button variant="secondary" size="sm">
            <Eye size={14} />
            <span className="ml-1.5">Zum Draft</span>
          </Button>
        </Link>
      </div>
    </Card>
  )
}

export default function DraftLobbyNeu() {
  const [team1, setTeam1] = useState('Team 1')
  const [team2, setTeam2] = useState('Team 2')
  const [preset, setPreset] = useState<DraftPreset>('competitive_2ban')
  const [zugSekunden, setZugSekunden] = useState(30)
  const [reserveSekunden, setReserveSekunden] = useState(120)
  const anlegen = useCreateDraftLobby()

  if (anlegen.data) return (
    <div className="mx-auto max-w-2xl px-4 py-10">
      <Fertig zugang={anlegen.data} />
    </div>
  )

  const absenden = (e: React.FormEvent) => {
    e.preventDefault()
    anlegen.mutate({
      team1_name: team1.trim(),
      team2_name: team2.trim(),
      preset,
      round_seconds: zugSekunden,
      reserve_seconds: reserveSekunden,
    })
  }

  const feld = 'w-full rounded-lg border border-border bg-background/60 px-3 py-2 text-sm text-foreground outline-none focus:border-border-strong'

  return (
    <div className="mx-auto max-w-2xl px-4 py-10">
      <div className="mb-8">
        <div className="flex items-center gap-2 text-primary">
          <Swords size={18} />
          <span className="text-xs font-semibold tracking-widest">DRAFT</span>
        </div>
        <h1 className="mt-2 text-3xl text-foreground">Pick &amp; Ban</h1>
        <p className="mt-2 text-sm text-muted">
          Lobby aufmachen, Links an die Captains schicken, draften. Kein Konto nötig.
        </p>
      </div>

      <form onSubmit={absenden}>
        <Card className="p-6">
          <div className="grid gap-4 sm:grid-cols-2">
            <label className="block">
              <span className="mb-1.5 block text-xs font-semibold text-muted">Team 1</span>
              <input className={feld} value={team1} maxLength={40} onChange={(e) => setTeam1(e.target.value)} />
            </label>
            <label className="block">
              <span className="mb-1.5 block text-xs font-semibold text-muted">Team 2</span>
              <input className={feld} value={team2} maxLength={40} onChange={(e) => setTeam2(e.target.value)} />
            </label>
          </div>

          <div className="mt-6">
            <span className="mb-2 block text-xs font-semibold text-muted">Ablauf</span>
            <div className="grid gap-2 sm:grid-cols-3">
              {PRESETS.map((p) => (
                <button
                  key={p.id}
                  type="button"
                  onClick={() => setPreset(p.id)}
                  className={`rounded-lg border p-3 text-left transition-colors ${
                    preset === p.id
                      ? 'border-primary bg-primary-soft'
                      : 'border-border bg-background/40 hover:border-border-hover'
                  }`}
                >
                  <div className={`text-sm font-semibold ${preset === p.id ? 'text-primary' : 'text-foreground'}`}>
                    {p.titel}
                  </div>
                  <div className="mt-0.5 text-xs text-muted">{p.erklaerung}</div>
                </button>
              ))}
            </div>
          </div>

          <div className="mt-6 grid gap-4 sm:grid-cols-2">
            <label className="block">
              <span className="mb-1.5 block text-xs font-semibold text-muted">Sekunden pro Zug</span>
              <input
                className={feld}
                type="number"
                min={10}
                max={300}
                value={zugSekunden}
                onChange={(e) => setZugSekunden(Number(e.target.value))}
              />
            </label>
            <label className="block">
              <span className="mb-1.5 block text-xs font-semibold text-muted">Reserve je Team</span>
              <input
                className={feld}
                type="number"
                min={0}
                max={600}
                value={reserveSekunden}
                onChange={(e) => setReserveSekunden(Number(e.target.value))}
              />
              <span className="mt-1 block text-xs text-muted">
                Wird angeknabbert, wenn ein Zug länger dauert.
              </span>
            </label>
          </div>

          {anlegen.isError && (
            <p className="mt-4 text-sm text-danger">{(anlegen.error as Error).message}</p>
          )}

          <div className="mt-6">
            <Button type="submit" disabled={anlegen.isPending}>
              {anlegen.isPending ? 'Wird angelegt…' : 'Lobby aufmachen'}
            </Button>
          </div>
        </Card>
      </form>
    </div>
  )
}
