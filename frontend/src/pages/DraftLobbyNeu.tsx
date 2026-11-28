import { useState, type FormEvent } from 'react'
import { useNavigate } from 'react-router-dom'
import TeamModeNav from '@/components/draft/TeamModeNav'
import { ArrowUpRight, ChevronLeft, ChevronRight } from 'lucide-react'
import { useDraftRaumAnlegen } from '@/hooks/useDraftLobby'

const BANS_OPTIONEN = [0, 1, 2, 3, 4, 5, 6]
const TIMER_OPTIONEN = [0, 30, 45, 60, 90]

function Einstellung({
  label, value, options, onChange, suffix, hint,
}: {
  label: string
  value: number
  options: number[]
  onChange: (value: number) => void
  suffix: string
  hint: string
}) {
  const index = options.indexOf(value)
  return (
    <fieldset className="pulp-setting">
      <legend className="pulp-overline">{label}</legend>
      <div className="pulp-setting__dial">
        <button type="button" disabled={index <= 0} aria-label={`${label} verringern`}
          onClick={() => onChange(options[index - 1])}>
          <ChevronLeft size={23} />
        </button>
        <strong aria-live="polite">{value === 0 && suffix === 's' ? 'AUS' : value}</strong>
        <button type="button" disabled={index >= options.length - 1} aria-label={`${label} erhöhen`}
          onClick={() => onChange(options[index + 1])}>
          <ChevronRight size={23} />
        </button>
      </div>
      <span className="pulp-setting__hint">{hint} {value > 0 && suffix ? `· ${value}${suffix}` : ''}</span>
    </fieldset>
  )
}

export default function DraftLobbyNeu() {
  const navigate = useNavigate()
  const anlegen = useDraftRaumAnlegen()
  const [segment, setSegment] = useState<'anlegen' | 'beitreten'>('anlegen')
  const [team1, setTeam1] = useState('Amber Hand')
  const [team2, setTeam2] = useState('Sapphire Flame')
  const [bans, setBans] = useState(2)
  const [timer, setTimer] = useState(30)
  const [beitrittsCode, setBeitrittsCode] = useState('')

  const draftStarten = (event: FormEvent) => {
    event.preventDefault()
    anlegen.mutate({
      team1_name: team1.trim() || undefined,
      team2_name: team2.trim() || undefined,
      bans_per_team: bans,
      round_seconds: timer,
    }, { onSuccess: (raum) => navigate(`/draft/${raum.code}`) })
  }

  const beitreten = (event: FormEvent) => {
    event.preventDefault()
    const code = beitrittsCode.trim().toUpperCase()
    if (code.length >= 4) navigate(`/draft/${code}`)
  }

  return (
    <div className="draft-pulp">


      <div className="draft-pulp__layout">
        <section className="draft-pulp__poster" aria-label="Deadlock Draft Arena">
          <div className="draft-pulp__city" aria-hidden="true" />
          <div className="draft-pulp__art" aria-hidden="true">
            <div className="draft-pulp__moon" />
            <div className="draft-pulp__skyline draft-pulp__skyline--back" />
            <div className="draft-pulp__skyline draft-pulp__skyline--front" />
          </div>
          <div className="draft-pulp__poster-head">
            <div className="draft-pulp__seal" aria-hidden="true">✶</div>
            <span className="draft-pulp__poster-kicker">DIE DEUTSCHE DEADLOCK COMMUNITY PRÄSENTIERT</span>
            <h1><span>DER</span><strong>DRAFT</strong></h1>
            <span className="draft-pulp__rule" />
            <p className="draft-pulp__tagline">Zwei Seiten. Zwölf Helden.<br />Eine Nacht, die alles entscheidet.</p>
          </div>
          <div className="draft-pulp__poster-footer" aria-hidden="true">
            <span>DIE STADT SCHLÄFT NIE</span>
            <span className="draft-pulp__poster-sigil">✦</span>
            <span>DIE GÖTTER SEHEN ZU</span>
          </div>
        </section>

        <section className="draft-pulp__desk" aria-label="Draft-Raum einrichten">
          <div className="draft-pulp__desk-inner">
            <TeamModeNav />
            <div className="draft-pulp__intro">
              <div className="pulp-overline">PRIVATES SCRIM · EIGENE LOBBY</div>
              <h2>{segment === 'anlegen' ? <>BEREIT FÜR<br /><em>DEN DRAFT?</em></> : <>ZURÜCK IN<br /><em>DEN KAMPF.</em></>}</h2>
              <p>{segment === 'anlegen'
                ? 'Stelle die Regeln auf, gib beiden Seiten einen Namen und eröffne die Auswahl. Der Raum gehört den Captains.'
                : 'Mit dem Einladungscode gelangst du direkt an den Draft-Tisch.'}</p>
            </div>

            <div className="draft-pulp__tabs" role="tablist" aria-label="Raummodus">
              <button type="button" role="tab" aria-selected={segment === 'anlegen'} onClick={() => setSegment('anlegen')}>
                01 <span>Raum erstellen</span>
              </button>
              <button type="button" role="tab" aria-selected={segment === 'beitreten'} onClick={() => setSegment('beitreten')}>
                02 <span>Raum betreten</span>
              </button>
            </div>

            {segment === 'beitreten' ? (
              <form className="draft-pulp__form" onSubmit={beitreten}>
                <label className="pulp-field">
                  <span className="pulp-overline">DEIN EINLADUNGSCODE</span>
                  <input autoFocus value={beitrittsCode} onChange={(e) => setBeitrittsCode(e.target.value.toUpperCase())}
                    maxLength={8} placeholder="XXXXXX" autoCapitalize="characters" spellCheck={false}
                    className="pulp-field__code" />
                </label>
                <button type="submit" disabled={beitrittsCode.trim().length < 4} className="draft-pulp__submit">
                  Raum betreten <ArrowUpRight aria-hidden="true" size={22}/>
                </button>
                <p className="draft-pulp__footnote">Den Einladungscode erhältst du vom Veranstalter.</p>
              </form>
            ) : (
              <form className="draft-pulp__form" onSubmit={draftStarten}>
                <div className="draft-pulp__teams">
                  <label className="pulp-field">
                    <span className="pulp-overline"><i className="pulp-marker pulp-marker--amber" /> TEAM I · AMBER HAND</span>
                    <input value={team1} onChange={(e) => setTeam1(e.target.value)} maxLength={40}
                      placeholder="Team 1" aria-label="Team 1 Name" />
                  </label>
                  <span className="draft-pulp__vs" aria-hidden="true">VS</span>
                  <label className="pulp-field">
                    <span className="pulp-overline"><i className="pulp-marker pulp-marker--sapphire" /> TEAM II · SAPPHIRE FLAME</span>
                    <input value={team2} onChange={(e) => setTeam2(e.target.value)} maxLength={40}
                      placeholder="Team 2" aria-label="Team 2 Name" />
                  </label>
                </div>

                <div className="draft-pulp__configuration">
                  <div className="draft-pulp__format">
                    <span className="pulp-overline">SPIELFORMAT</span>
                    <strong>6 <span>VS</span> 6</strong>
                    <span className="draft-pulp__hint">Standard · 12 Picks</span>
                  </div>
                  <Einstellung label="BANS PRO TEAM" value={bans} options={BANS_OPTIONEN}
                    onChange={setBans} suffix="" hint="Gesperrte Helden" />
                  <Einstellung label="ZUGZEIT" value={timer} options={TIMER_OPTIONEN}
                    onChange={setTimer} suffix="s" hint="Pro Entscheidung" />
                </div>

                    {anlegen.isError && <p role="alert" className="draft-pulp__error">
                  Der Raum konnte nicht erstellt werden. Bitte erneut versuchen.
                </p>}
                <button type="submit" disabled={anlegen.isPending} className="draft-pulp__submit">
                  {anlegen.isPending ? 'Raum wird erstellt …' : 'Draft-Raum erstellen'}
                  <ArrowUpRight aria-hidden="true" size={24} />
                </button>
                <p className="draft-pulp__footnote">
                  6 gegen 6 · {bans} Bans pro Team · {timer === 0 ? 'Ohne Timer' : `${timer} Sekunden pro Zug`}<br />
                  Danach erhalten die Captains einen gemeinsamen Raum-Code.
                </p>
              </form>
            )}
          </div>

        </section>
      </div>
    </div>
  )
}
