import { useState } from 'react'
import { Link, useNavigate, useParams } from 'react-router-dom'
import { AlertCircle, ArrowRight, Check, Copy, Crown, LogOut, Search, Sparkles, Users, X } from 'lucide-react'
import TeamModeNav from '@/components/draft/TeamModeNav'
import { useDraftHeroList } from '@/hooks/useDraftLobby'
import { CompApiError, useCompRoom, useCreateComp, useJoinComp, useLeaveComp, useRemoveComp, useSaveComp } from '@/hooks/useCompFinder'
import { compShareUrl, compositionText, nextPriority, normalizeCompCode, preferenceList, preferenceMap, priorityLabel } from '@/hooks/compState'
import type { CompComposition, CompMember, CompPriority, CompRoom } from '@/types/comp'
import type { DraftHero } from '@/types/draft'
import '@/comp.css'

function ErrorNote({ error }: { error: unknown }) {
  if (!error) return null
  return <p role="alert" className="mt-3 flex items-start gap-2 text-sm text-red-300"><AlertCircle size={17} className="mt-0.5 shrink-0" />{error instanceof Error ? error.message : 'Etwas ist schiefgelaufen. Bitte erneut versuchen.'}</p>
}

function CopyButton({ text, label }: { text: string; label: string }) {
  const [copied, setCopied] = useState(false)
  const [failed, setFailed] = useState(false)
  async function copy() {
    try {
      await navigator.clipboard.writeText(text)
      setCopied(true)
      setFailed(false)
    } catch { setFailed(true) }
  }
  return <div>
    <button type="button" className="comp-button comp-button-secondary" onClick={copy} aria-label={label}>
      {copied ? <Check size={16} /> : <Copy size={16} />}{copied ? 'Kopiert' : label}
    </button>
    {failed && <label className="mt-2 block text-xs text-muted">Kopieren nicht erlaubt – Text markieren und kopieren:<textarea readOnly value={text} onFocus={e => e.currentTarget.select()} className="comp-input mt-1" rows={3} /></label>}
  </div>
}

function Portrait({ hero, name }: { hero?: DraftHero; name: string }) {
  const [failed, setFailed] = useState(false)
  const url = hero?.image_url?.trim()
  return url && !failed ? <img src={url} alt="" loading="lazy" onError={() => setFailed(true)} className="comp-portrait" /> : <span className="comp-portrait comp-portrait-fallback" aria-hidden="true">{name.slice(0, 1)}</span>
}

function Landing() {
  const navigate = useNavigate()
  const create = useCreateComp()
  const [name, setName] = useState('')
  const [code, setCode] = useState('')
  const [codeError, setCodeError] = useState('')
  return <div className="comp-page">
    <TeamModeNav />
    <header className="mx-auto mb-10 max-w-2xl text-center">
      <span className="comp-eyebrow"><Users size={15} /> TEAM-PLANUNG · 1–6 SPIELER</span>
      <h1 className="mt-4 font-display text-4xl font-black tracking-tight sm:text-6xl">Eure Helden.<br /><span className="text-primary">Eine passende Comp.</span></h1>
      <p className="mx-auto mt-5 max-w-xl text-base leading-relaxed text-muted">Jeder wählt, was er spielen kann und worauf er Lust hat. Der Comp-Finder verteilt die Helden ohne Doppelbelegung und zeigt eure bestpassenden Aufstellungen.</p>
    </header>
    <div className="mx-auto grid max-w-4xl gap-5 md:grid-cols-2">
      <section className="comp-panel p-6 sm:p-8">
        <span className="comp-eyebrow">NEUES TEAM</span>
        <h2 className="mt-3 text-2xl font-bold">Lobby erstellen</h2>
        <p className="mt-2 text-sm leading-relaxed text-muted">Du erhältst einen Code und einen Link für deine Mitspieler. Kein Discord-Login nötig.</p>
        <form className="mt-6 space-y-4" onSubmit={e => { e.preventDefault(); create.mutate(name.trim(), { onSuccess: room => navigate(`/comp/${room.code}`) }) }}>
          <label className="block text-sm font-semibold">Dein Name<input autoComplete="nickname" className="comp-input mt-2" maxLength={32} required value={name} onChange={e => setName(e.target.value)} placeholder="Wie nennt dich dein Team?" /></label>
          <button className="comp-button w-full justify-center" disabled={!name.trim() || create.isPending}>{create.isPending ? 'Lobby wird erstellt …' : 'Lobby erstellen'}<ArrowRight size={17} /></button>
          <ErrorNote error={create.error} />
        </form>
      </section>
      <section className="comp-panel p-6 sm:p-8">
        <span className="comp-eyebrow">TEAM SCHON DA?</span>
        <h2 className="mt-3 text-2xl font-bold">Mit Code beitreten</h2>
        <p className="mt-2 text-sm leading-relaxed text-muted">Öffne die Lobby deines Teams. Dort wählst du deinen Namen und deine Helden.</p>
        <form className="mt-6 space-y-4" onSubmit={e => { e.preventDefault(); const normalized = normalizeCompCode(code); if (!normalized) { setCodeError('Bitte einen gültigen 8-stelligen Comp-Code oder Lobby-Link eingeben.'); return } navigate(`/comp/${normalized}`) }}>
          <label className="block text-sm font-semibold">Lobby-Code oder Link<input className="comp-input mt-2" required maxLength={512} value={code} onChange={e => { setCode(e.target.value); setCodeError('') }} placeholder="ABCD2345" autoCapitalize="characters" spellCheck={false} /></label>
          <button className="comp-button comp-button-secondary w-full justify-center" disabled={!code.trim()}>Lobby öffnen<ArrowRight size={17} /></button>
          <ErrorNote error={codeError ? new Error(codeError) : null} />
        </form>
      </section>
    </div>
    <div className="mx-auto mt-8 grid max-w-4xl gap-5 text-sm text-muted sm:grid-cols-3">
      <p><strong className="mb-1 block text-foreground">01 · Helden markieren</strong>Spielbar, bevorzugt oder höchste Priorität – du entscheidest.</p>
      <p><strong className="mb-1 block text-foreground">02 · Wünsche verteilen</strong>Ein Held pro Spieler, jeder Held nur einmal. Auch mit weniger als sechs Spielern.</p>
      <p><strong className="mb-1 block text-foreground">03 · Alternativen vergleichen</strong>Bis zu zehn unterschiedliche Helden-Teams, nach Wunschpunkten sortiert.</p>
    </div>
    <p className="mx-auto mt-8 max-w-3xl text-center text-xs leading-relaxed text-muted">Die Punkte bewerten eure Auswahl, nicht Meta, Rollen oder Helden-Synergien. Lobbys sind 24 Stunden gültig. Jeder mit dem Link kann Namen, Heldenwünsche und Aufstellungen sehen; nur dein Browser kann deine Auswahl ändern.</p>
  </div>
}

function JoinForm({ room }: { room: CompRoom }) {
  const join = useJoinComp(room.code)
  const [name, setName] = useState('')
  const full = room.members.length >= 6
  return <section className="comp-panel mb-6 p-5">
    <h2 className="text-lg font-bold">{full ? 'Alle sechs Plätze sind belegt' : 'Deinen Spielerplatz nehmen'}</h2>
    <p className="mt-1 text-sm text-muted">{full ? 'Du kannst die Aufstellungen mitverfolgen. Der Gastgeber kann einen nicht mehr benötigten Platz freigeben.' : 'Du schaust gerade zu. Tritt bei, um deine eigenen Helden auszuwählen.'}</p>
    {!full && <form className="mt-4 flex flex-wrap gap-3" onSubmit={e => { e.preventDefault(); join.mutate(name.trim()) }}>
      <input aria-label="Dein Name" autoComplete="nickname" className="comp-input min-w-0 flex-1" maxLength={32} required value={name} onChange={e => setName(e.target.value)} placeholder="Dein Name" />
      <button className="comp-button" disabled={!name.trim() || join.isPending}>{join.isPending ? 'Beitritt …' : 'Mitspielen'}<ArrowRight size={16} /></button>
    </form>}
    <ErrorNote error={join.error} />
  </section>
}

function Members({ room }: { room: CompRoom }) {
  const remove = useRemoveComp(room.code)
  return <section className="mb-6" aria-label="Spieler in der Lobby">
    <div className="mb-3 flex items-center justify-between text-sm"><h2 className="font-semibold">Euer Team <span className="ml-2 text-muted">{room.members.length}/6</span></h2><span className="text-xs text-muted">{room.members.filter(m => m.preferences.length > 0).length} mit gespeicherter Auswahl</span></div>
    <div className="grid grid-cols-2 gap-2 sm:grid-cols-3 xl:grid-cols-6">
      {Array.from({ length: 6 }, (_, i) => {
        const member = room.members[i]
        if (!member) return <div key={`empty-${i}`} className="comp-seat comp-seat-empty"><Users size={18} /><span>Freier Platz</span></div>
        return <div key={member.id} className={`comp-seat ${member.id === room.you ? 'comp-seat-you' : ''}`}>
          <div className="flex w-full items-center gap-1.5">
            {member.id === room.host_member_id && <Crown size={14} className="shrink-0 text-primary" aria-label="Gastgeber" />}
            <span className="min-w-0 flex-1 truncate font-semibold" title={member.name}>{member.name}</span>
            {room.you === room.host_member_id && member.id !== room.you && <button type="button" disabled={remove.isPending} className="rounded p-1 text-muted hover:text-red-300" aria-label={`${member.name} aus Lobby entfernen`} onClick={() => { if (window.confirm(`${member.name} aus dieser Lobby entfernen?`)) remove.mutate(member.id) }}><X size={14} /></button>}
          </div>
          <span className="text-xs text-muted">{member.id === room.you ? 'Du · ' : ''}{member.preferences.length ? `${member.preferences.length} Helden` : 'Wählt noch'}</span>
        </div>
      })}
    </div>
    <ErrorNote error={remove.error} />
  </section>
}

function Editor({ room, member, heroes }: { room: CompRoom; member: CompMember; heroes: DraftHero[] }) {
  const save = useSaveComp(room.code)
  const [draft, setDraft] = useState(() => ({ revision: member.revision, preferences: preferenceMap(member.preferences), baseline: JSON.stringify(preferenceList(preferenceMap(member.preferences))) }))
  const [search, setSearch] = useState('')
  const [selectedOnly, setSelectedOnly] = useState(false)
  const list = preferenceList(draft.preferences)
  const dirty = JSON.stringify(list) !== draft.baseline
  const stale = member.revision !== draft.revision
  const visible = heroes.filter(h => h.name.toLocaleLowerCase().includes(search.toLocaleLowerCase()) && (!selectedOnly || draft.preferences[h.name] !== undefined))
  const unavailable = list.filter(p => !heroes.some(h => h.name === p.hero_name))
  function cycle(name: string) {
    setDraft(old => {
      const preferences = { ...old.preferences }
      const next = nextPriority(preferences[name])
      if (next === undefined) delete preferences[name]
      else preferences[name] = next
      return { ...old, preferences }
    })
  }
  function adopt(current: CompMember) {
    const preferences = preferenceMap(current.preferences)
    setDraft({ revision: current.revision, preferences, baseline: JSON.stringify(preferenceList(preferences)) })
  }
  return <section className="comp-panel overflow-hidden">
    <div className="border-b border-border p-5">
      <div className="flex flex-wrap items-center justify-between gap-2"><h2 className="text-xl font-bold">Deine Helden</h2><span className="text-xs text-muted">{list.length} ausgewählt</span></div>
      <p className="mt-2 text-sm leading-relaxed text-muted">Klicke durch die Stufen: nicht ausgewählt → spielbar → bevorzugt → höchste Priorität → aus.</p>
      <div className="mt-4 flex flex-wrap gap-2">
        {([0, 1, 2] as const).map(priority => <span key={priority} className={`comp-priority comp-priority-${priority}`}>{priority} P. · {priorityLabel(priority)}</span>)}
      </div>
    </div>
    <div className="p-5">
      {stale && <div role="alert" className="mb-4 rounded-lg border border-amber-400/40 bg-amber-400/10 p-3 text-sm text-amber-200">Deine gespeicherte Auswahl wurde in einem anderen Tab geändert. Deine lokalen Änderungen bleiben erhalten.<button type="button" className="mt-2 block font-semibold underline" onClick={() => { if (!dirty || window.confirm('Lokale Änderungen verwerfen und den aktuellen Stand übernehmen?')) adopt(member) }}>Aktuellen Stand übernehmen</button></div>}
      <div className="flex flex-wrap items-center gap-3">
        <label className="relative min-w-40 flex-1"><Search size={16} className="absolute left-3 top-3 text-muted" /><input aria-label="Helden suchen" className="comp-input pl-9" value={search} onChange={e => setSearch(e.target.value)} placeholder="Held suchen …" /></label>
        <label className="flex items-center gap-2 text-xs text-muted"><input type="checkbox" checked={selectedOnly} onChange={e => setSelectedOnly(e.target.checked)} />Nur ausgewählte</label>
      </div>
      <div className="my-3 flex flex-wrap gap-x-4 gap-y-2 text-xs">
        <button type="button" disabled={save.isPending} className="text-primary hover:underline" onClick={() => setDraft(old => ({ ...old, preferences: { ...Object.fromEntries(heroes.map(h => [h.name, 0 as CompPriority])), ...old.preferences } }))}>Alle als spielbar ergänzen</button>
        <button type="button" disabled={save.isPending || !list.length} className="text-muted hover:text-foreground disabled:opacity-40" onClick={() => setDraft(old => ({ ...old, preferences: {} }))}>Auswahl leeren</button>
      </div>
      {unavailable.length > 0 && <div className="mb-4 rounded-lg border border-amber-400/30 p-3 text-xs text-amber-200">Nicht mehr in der verfügbaren Heldenliste: {unavailable.map(p => p.hero_name).join(', ')}.<button type="button" className="ml-2 underline" onClick={() => setDraft(old => ({ ...old, preferences: Object.fromEntries(Object.entries(old.preferences).filter(([name]) => heroes.some(h => h.name === name))) }))}>Aus Auswahl entfernen</button></div>}
      <div className="comp-hero-grid" aria-label="Helden und Prioritäten">
        {visible.map(hero => {
          const priority = draft.preferences[hero.name]
          return <button key={hero.name} type="button" disabled={save.isPending} className={`comp-hero ${priority === undefined ? '' : `comp-hero-${priority}`}`} aria-pressed={priority !== undefined} aria-label={`${hero.name}: ${priorityLabel(priority)}${priority === undefined ? '' : `, ${priority} Punkte`}. Klicken für nächste Stufe.`} title={`${hero.name} · ${priorityLabel(priority)}`} onClick={() => cycle(hero.name)}>
            <Portrait hero={hero} name={hero.name} />
            <span className="comp-hero-name">{hero.name}</span>
            <span className="comp-hero-value">{priority === undefined ? '—' : `${priority} P.`}</span>
          </button>
        })}
      </div>
      {visible.length === 0 && <p className="py-8 text-center text-sm text-muted">Keine Helden für diesen Filter.</p>}
    </div>
    <div className="comp-savebar border-t border-border p-4">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <span role="status" className={`text-xs ${dirty ? 'text-amber-200' : 'text-muted'}`}>{save.isPending ? 'Wird gespeichert …' : dirty ? 'Ungespeichert – dein Team sieht noch den letzten Stand.' : 'Deine Auswahl ist gespeichert.'}</span>
        <button className="comp-button" type="button" disabled={!dirty || stale || save.isPending || unavailable.length > 0} onClick={() => save.mutate({ revision: draft.revision, preferences: list }, { onSuccess: updated => { const current = updated.members.find(m => m.id === member.id); if (current) adopt(current) } })}><Check size={16} />Auswahl speichern</button>
      </div>
      <ErrorNote error={save.error} />
    </div>
  </section>
}

function CompositionCard({ room, composition, rank, heroes }: { room: CompRoom; composition: CompComposition; rank: number; heroes: DraftHero[] }) {
  return <div className="p-4 sm:p-5">
    <div className="mb-4 flex items-end justify-between gap-2">
      <div><span className="comp-eyebrow">{rank === 1 ? 'BESTER WUNSCH-FIT' : `ALTERNATIVE ${rank}`}</span><p className="mt-1 text-xs text-muted">{composition.top_priority_count}× höchste Priorität</p></div>
      <p className="text-right"><strong className="text-3xl font-black text-primary">{composition.score}</strong><span className="text-sm text-muted"> / {room.results.max_score} P.</span></p>
    </div>
    <div className="space-y-2">
      {composition.assignments.map(assignment => {
        const member = room.members[assignment.player_index]
        return <div key={assignment.player_index} className={`flex items-center gap-3 rounded-lg border p-2 ${member?.id === room.you ? 'border-primary/30 bg-primary/5' : 'border-border bg-black/10'}`}>
          <div className="h-10 w-10 shrink-0 overflow-hidden rounded-md"><Portrait hero={heroes.find(h => h.name === assignment.hero_name)} name={assignment.hero_name} /></div>
          <div className="min-w-0 flex-1"><div className="truncate text-xs text-muted">{member?.name}{member?.id === room.you ? ' · Du' : ''}</div><div className="truncate text-sm font-semibold">{assignment.hero_name}</div></div>
          <span className={`comp-priority comp-priority-${assignment.priority}`}>{assignment.priority} P.</span>
        </div>
      })}
    </div>
    <div className="mt-4"><CopyButton text={compositionText(room, composition, rank)} label="Aufstellung kopieren" /></div>
  </div>
}

function Results({ room, heroes }: { room: CompRoom; heroes: DraftHero[] }) {
  const { results } = room
  const names = (indices: number[]) => indices.map(i => room.members[i]?.name ?? 'Spieler').join(', ')
  return <section aria-label="Passende Aufstellungen">
    <div className="mb-4 flex items-center gap-2"><Sparkles size={20} className="text-primary" /><h2 className="text-xl font-bold">Eure Aufstellungen</h2></div>
    <p className="mb-4 text-xs leading-relaxed text-muted">Summe der zugewiesenen Prioritäten. Bei Gleichstand zählen mehr höchste Prioritäten, danach eine stabile Heldenreihenfolge. Keine Meta- oder Synergie-Wertung.</p>
    {room.members.length < 6 && <p className="mb-4 rounded-lg border border-border bg-card p-3 text-xs text-muted">Vorschau für {room.members.length} Spieler. Weitere Mitspieler können jederzeit beitreten – die Aufstellungen werden dann neu berechnet.</p>}
    {results.waiting_for.length > 0 ? <div className="comp-panel p-6"><Users size={28} className="mb-3 text-primary" /><h3 className="font-bold">Es fehlen noch Heldenwünsche</h3><p className="mt-2 text-sm leading-relaxed text-muted">{names(results.waiting_for)}: Bitte mindestens einen verfügbaren Helden auswählen und speichern.</p></div> : results.conflict ? <div className="comp-panel border-amber-400/40 p-6"><AlertCircle size={28} className="mb-3 text-amber-200" /><h3 className="font-bold">Noch keine gültige Aufstellung</h3><p className="mt-2 text-sm leading-relaxed text-muted">{names(results.conflict.player_indices)} teilen sich nur {results.conflict.available_heroes.length} unterschiedliche Helden: {results.conflict.available_heroes.join(', ')}. Für diese {results.conflict.player_indices.length} Spieler reichen die freien Helden nicht.</p><p className="mt-3 text-sm text-amber-200">Mindestens einer von ihnen muss weitere spielbare Helden ergänzen. Ein weiterer Held ist ein Anfang; die neue Auswahl wird erneut geprüft.</p></div> : results.compositions.length > 0 ? <div className="space-y-3">
      {results.compositions.map((composition, i) => i === 0 ? <div key="best" className="comp-panel comp-best"><CompositionCard room={room} composition={composition} rank={1} heroes={heroes} /></div> : <details key={composition.assignments.map(a => a.hero_name).sort().join('|')} className="comp-panel"><summary className="cursor-pointer px-5 py-4 text-sm font-semibold">Alternative {i + 1}<span className="float-right text-primary">{composition.score}/{results.max_score} P.</span></summary><CompositionCard room={room} composition={composition} rank={i + 1} heroes={heroes} /></details>)}
      <p className="text-xs text-muted">{results.compositions.length} unterschiedliche Helden-Teams{results.compositions.length === 10 ? ' · die besten 10' : ''}. Identische Helden-Sets erscheinen nur einmal mit ihrer besten Spielerzuordnung.</p>
    </div> : <p className="comp-panel p-6 text-sm text-muted">Noch keine Aufstellungen vorhanden.</p>}
  </section>
}

function RoomPage({ code }: { code: string }) {
  const navigate = useNavigate()
  const lobby = useCompRoom(code)
  const heroQuery = useDraftHeroList()
  const leave = useLeaveComp(code)
  const room = lobby.data
  const heroes = [...(heroQuery.data?.heroes ?? [])].sort((a, b) => a.name.localeCompare(b.name))
  if ((lobby.error instanceof CompApiError && lobby.error.status === 404) || !room) return <div className="comp-page mx-auto max-w-xl text-center"><TeamModeNav /><h1 className="text-2xl font-bold">{lobby.isPending ? 'Lobby wird geladen …' : 'Lobby nicht erreichbar'}</h1><ErrorNote error={lobby.error} />{lobby.error && <button className="comp-button mt-5" onClick={() => lobby.refetch()}>Erneut versuchen</button>}<Link to="/comp" className="mt-6 block text-sm text-primary hover:underline">Zurück zum Comp-Finder</Link></div>
  const member = room.members.find(m => m.id === room.you)
  const share = compShareUrl(window.location.origin, import.meta.env.BASE_URL, room.code)
  return <div className="comp-page">
    <TeamModeNav />
    <header className="mb-6 flex flex-wrap items-start justify-between gap-5">
      <div><span className="comp-eyebrow">COMP-FINDER</span><h1 className="mt-2 font-display text-3xl font-black sm:text-4xl">Euer Team. Eure Helden.</h1><p className="mt-2 text-xs text-muted">Gültig bis {new Date(room.expires_at).toLocaleString('de-DE', { dateStyle: 'short', timeStyle: 'short' })} · {lobby.error ? 'Verbindung unterbrochen – letzter gespeicherter Stand' : 'Aktualisierung alle 2 Sekunden'}</p></div>
      <div className="comp-panel flex flex-wrap items-center gap-4 p-4"><div><span className="block text-[10px] font-bold tracking-widest text-muted">LOBBY-CODE</span><span className="select-all font-mono text-2xl font-bold tracking-widest text-primary">{room.code}</span></div><CopyButton text={share} label="Einladungslink" /></div>
    </header>
    {lobby.error && <div className="mb-5"><ErrorNote error={lobby.error} /><button className="mt-2 text-sm text-primary underline" onClick={() => lobby.refetch()}>Verbindung erneut prüfen</button></div>}
    <Members room={room} />
    {!member && <JoinForm room={room} />}
    {room.unavailable_heroes.length > 0 && <p role="status" className="mb-5 rounded-lg border border-amber-400/30 p-3 text-sm text-amber-200">Aktuell nicht verfügbare Helden werden nicht zugeteilt: {room.unavailable_heroes.join(', ')}. Betroffene Spieler sollten ihre Auswahl aktualisieren.</p>}
    <div className={`grid items-start gap-6 ${member ? 'xl:grid-cols-[minmax(0,1.5fr)_minmax(330px,1fr)]' : 'mx-auto max-w-2xl'}`}>
      {member && (heroQuery.isPending ? <div className="comp-panel p-8 text-muted">Helden werden geladen …</div> : heroQuery.error || !heroes.length ? <div className="comp-panel p-6"><ErrorNote error={heroQuery.error ?? new Error('Die Heldenliste ist leer.')} /><button className="comp-button mt-4" onClick={() => heroQuery.refetch()}>Helden neu laden</button></div> : <Editor key={member.id} room={room} member={member} heroes={heroes} />)}
      <Results room={room} heroes={heroes} />
    </div>
    <footer className="mt-8 flex flex-wrap items-center justify-between gap-4 border-t border-border pt-5">
      <p className="max-w-2xl text-xs leading-relaxed text-muted">Link und Code geben Einblick in die Lobby, aber keine Kontrolle über fremde Spielerplätze. Dein Platz bleibt bei einem Neuladen dieses Tabs erhalten. Zum Wechsel des Browsers vorher austreten; der Gastgeber kann verwaiste Plätze freigeben.</p>
      {member && <button type="button" className="comp-button comp-button-secondary" disabled={leave.isPending} onClick={() => { if (window.confirm('Lobby verlassen? Dein Spielerplatz und deine gespeicherte Auswahl werden entfernt.')) leave.mutate(undefined, { onSuccess: () => navigate('/comp') }) }}><LogOut size={15} />Lobby verlassen</button>}
      <ErrorNote error={leave.error} />
    </footer>
  </div>
}

export default function CompFinder() {
  const { code } = useParams<{ code: string }>()
  return code ? <RoomPage key={code.toUpperCase()} code={code.toUpperCase()} /> : <Landing />
}
