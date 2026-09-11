import type {
  DraftAktion,
  DraftHero,
  DraftPhase,
  DraftRaumAnlegen,
  DraftRaumCode,
  DraftRaumZustand,
  DraftClaimAntwort,
  DraftSequenzSchritt,
  DraftTeamSeite,
} from '@/types/draft'

const SPEICHER_RAUM = 'draftmock:raum:'
const SPEICHER_ZUSCHAUER = 'draftmock:zuschauer'
const CODE_ALPHABET = 'ABCDEFGHJKLMNPQRSTUVWXYZ23456789'
const ZUSCHAUER_FENSTER_MS = 20_000
const LOBBY_DAUER_MS = 1_500

export class DraftMockFehler extends Error {
  status: number
  constructor(status: number, message: string) {
    super(message)
    this.status = status
  }
}

interface MockRaum {
  code: string
  team1Name: string
  team2Name: string
  bansPerTeam: number
  roundSeconds: number
  team1Viewer: string | null
  team2Viewer: string | null
  team1Token: string | null
  team2Token: string | null
  team1Ready: boolean
  team2Ready: boolean
  phase: DraftPhase
  currentActionIndex: number
  deadlineAt: string | null
  actions: DraftAktion[]
  lobbyStatus: string
  lobbyJoinCode: string | null
  lobbyError: string | null
  lobbyRequestedAt: number | null
  rematchOf: string | null
}

const HERO_FIXTURES: { id: number; name: string; portrait: string; card: string }[] = [
  { id: 6, name: "Abrams", portrait: "https://assets-bucket.deadlock-api.com/assets-api-res/images/heroes/bull_sm.webp", card: "https://assets-bucket.deadlock-api.com/assets-api-res/images/heroes/bull_card.webp" },
  { id: 77, name: "Apollo", portrait: "https://assets-bucket.deadlock-api.com/assets-api-res/images/heroes/fencer_sm.webp", card: "https://assets-bucket.deadlock-api.com/assets-api-res/images/heroes/fencer_card.webp" },
  { id: 15, name: "Bebop", portrait: "https://assets-bucket.deadlock-api.com/assets-api-res/images/heroes/bebop_sm.webp", card: "https://assets-bucket.deadlock-api.com/assets-api-res/images/heroes/bebop_card.webp" },
  { id: 72, name: "Billy", portrait: "https://assets-bucket.deadlock-api.com/assets-api-res/images/heroes/punkgoat_sm.webp", card: "https://assets-bucket.deadlock-api.com/assets-api-res/images/heroes/punkgoat_card.webp" },
  { id: 16, name: "Calico", portrait: "https://assets-bucket.deadlock-api.com/assets-api-res/images/heroes/nano_sm.webp", card: "https://assets-bucket.deadlock-api.com/assets-api-res/images/heroes/nano_card.webp" },
  { id: 81, name: "Celeste", portrait: "https://assets-bucket.deadlock-api.com/assets-api-res/images/heroes/unicorn_sm.webp", card: "https://assets-bucket.deadlock-api.com/assets-api-res/images/heroes/unicorn_card.webp" },
  { id: 64, name: "Drifter", portrait: "https://assets-bucket.deadlock-api.com/assets-api-res/images/heroes/drifter_sm.webp", card: "https://assets-bucket.deadlock-api.com/assets-api-res/images/heroes/drifter_card.webp" },
  { id: 11, name: "Dynamo", portrait: "https://assets-bucket.deadlock-api.com/assets-api-res/images/heroes/sumo_sm.webp", card: "https://assets-bucket.deadlock-api.com/assets-api-res/images/heroes/sumo_card.webp" },
  { id: 76, name: "Graves", portrait: "https://assets-bucket.deadlock-api.com/assets-api-res/images/heroes/necro_sm.webp", card: "https://assets-bucket.deadlock-api.com/assets-api-res/images/heroes/necro_card.webp" },
  { id: 17, name: "Grey Talon", portrait: "https://assets-bucket.deadlock-api.com/assets-api-res/images/heroes/archer_sm.webp", card: "https://assets-bucket.deadlock-api.com/assets-api-res/images/heroes/archer_card.webp" },
  { id: 13, name: "Haze", portrait: "https://assets-bucket.deadlock-api.com/assets-api-res/images/heroes/haze_sm.webp", card: "https://assets-bucket.deadlock-api.com/assets-api-res/images/heroes/haze_card.webp" },
  { id: 14, name: "Holliday", portrait: "https://assets-bucket.deadlock-api.com/assets-api-res/images/heroes/astro_sm.webp", card: "https://assets-bucket.deadlock-api.com/assets-api-res/images/heroes/astro_card.webp" },
  { id: 1, name: "Infernus", portrait: "https://assets-bucket.deadlock-api.com/assets-api-res/images/heroes/inferno_sm.webp", card: "https://assets-bucket.deadlock-api.com/assets-api-res/images/heroes/inferno_card.webp" },
  { id: 20, name: "Ivy", portrait: "https://assets-bucket.deadlock-api.com/assets-api-res/images/heroes/tengu_sm.webp", card: "https://assets-bucket.deadlock-api.com/assets-api-res/images/heroes/tengu_card.webp" },
  { id: 12, name: "Kelvin", portrait: "https://assets-bucket.deadlock-api.com/assets-api-res/images/heroes/kelvin_sm.webp", card: "https://assets-bucket.deadlock-api.com/assets-api-res/images/heroes/kelvin_card.webp" },
  { id: 4, name: "Lady Geist", portrait: "https://assets-bucket.deadlock-api.com/assets-api-res/images/heroes/spectre_sm.webp", card: "https://assets-bucket.deadlock-api.com/assets-api-res/images/heroes/spectre_card.webp" },
  { id: 31, name: "Lash", portrait: "https://assets-bucket.deadlock-api.com/assets-api-res/images/heroes/lash_sm.webp", card: "https://assets-bucket.deadlock-api.com/assets-api-res/images/heroes/lash_card.webp" },
  { id: 8, name: "McGinnis", portrait: "https://assets-bucket.deadlock-api.com/assets-api-res/images/heroes/engineer_sm.webp", card: "https://assets-bucket.deadlock-api.com/assets-api-res/images/heroes/engineer_card.webp" },
  { id: 63, name: "Mina", portrait: "https://assets-bucket.deadlock-api.com/assets-api-res/images/heroes/vampirebat_sm.webp", card: "https://assets-bucket.deadlock-api.com/assets-api-res/images/heroes/vampirebat_card.webp" },
  { id: 52, name: "Mirage", portrait: "https://assets-bucket.deadlock-api.com/assets-api-res/images/heroes/mirage_sm.webp", card: "https://assets-bucket.deadlock-api.com/assets-api-res/images/heroes/mirage_card.webp" },
  { id: 18, name: "Mo & Krill", portrait: "https://assets-bucket.deadlock-api.com/assets-api-res/images/heroes/digger_sm.webp", card: "https://assets-bucket.deadlock-api.com/assets-api-res/images/heroes/digger_card.webp" },
  { id: 67, name: "Paige", portrait: "https://assets-bucket.deadlock-api.com/assets-api-res/images/heroes/bookworm_sm.webp", card: "https://assets-bucket.deadlock-api.com/assets-api-res/images/heroes/bookworm_card.webp" },
  { id: 10, name: "Paradox", portrait: "https://assets-bucket.deadlock-api.com/assets-api-res/images/heroes/chrono_sm.webp", card: "https://assets-bucket.deadlock-api.com/assets-api-res/images/heroes/chrono_card.webp" },
  { id: 50, name: "Pocket", portrait: "https://assets-bucket.deadlock-api.com/assets-api-res/images/heroes/synth_sm.webp", card: "https://assets-bucket.deadlock-api.com/assets-api-res/images/heroes/synth_card.webp" },
  { id: 79, name: "Rem", portrait: "https://assets-bucket.deadlock-api.com/assets-api-res/images/heroes/familiar_sm.webp", card: "https://assets-bucket.deadlock-api.com/assets-api-res/images/heroes/familiar_card.webp" },
  { id: 2, name: "Seven", portrait: "https://assets-bucket.deadlock-api.com/assets-api-res/images/heroes/gigawatt_sm.webp", card: "https://assets-bucket.deadlock-api.com/assets-api-res/images/heroes/gigawatt_card.webp" },
  { id: 19, name: "Shiv", portrait: "https://assets-bucket.deadlock-api.com/assets-api-res/images/heroes/shiv_sm.webp", card: "https://assets-bucket.deadlock-api.com/assets-api-res/images/heroes/shiv_card.webp" },
  { id: 80, name: "Silver", portrait: "https://assets-bucket.deadlock-api.com/assets-api-res/images/heroes/werewolf_sm.webp", card: "https://assets-bucket.deadlock-api.com/assets-api-res/images/heroes/werewolf_card.webp" },
  { id: 60, name: "Sinclair", portrait: "https://assets-bucket.deadlock-api.com/assets-api-res/images/heroes/magician_sm.webp", card: "https://assets-bucket.deadlock-api.com/assets-api-res/images/heroes/magician_card.webp" },
  { id: 69, name: "The Doorman", portrait: "https://assets-bucket.deadlock-api.com/assets-api-res/images/heroes/doorman_sm.webp", card: "https://assets-bucket.deadlock-api.com/assets-api-res/images/heroes/doorman_card.webp" },
  { id: 65, name: "Venator", portrait: "https://assets-bucket.deadlock-api.com/assets-api-res/images/heroes/priest_sm.webp", card: "https://assets-bucket.deadlock-api.com/assets-api-res/images/heroes/priest_card.webp" },
  { id: 66, name: "Victor", portrait: "https://assets-bucket.deadlock-api.com/assets-api-res/images/heroes/frank_sm.webp", card: "https://assets-bucket.deadlock-api.com/assets-api-res/images/heroes/frank_card.webp" },
  { id: 3, name: "Vindicta", portrait: "https://assets-bucket.deadlock-api.com/assets-api-res/images/heroes/hornet_sm.webp", card: "https://assets-bucket.deadlock-api.com/assets-api-res/images/heroes/hornet_card.webp" },
  { id: 35, name: "Viscous", portrait: "https://assets-bucket.deadlock-api.com/assets-api-res/images/heroes/viscous_sm.webp", card: "https://assets-bucket.deadlock-api.com/assets-api-res/images/heroes/viscous_card.webp" },
  { id: 58, name: "Vyper", portrait: "https://assets-bucket.deadlock-api.com/assets-api-res/images/heroes/kali_sm.webp", card: "https://assets-bucket.deadlock-api.com/assets-api-res/images/heroes/kali_card.webp" },
  { id: 25, name: "Warden", portrait: "https://assets-bucket.deadlock-api.com/assets-api-res/images/heroes/warden_sm.webp", card: "https://assets-bucket.deadlock-api.com/assets-api-res/images/heroes/warden_card.webp" },
  { id: 7, name: "Wraith", portrait: "https://assets-bucket.deadlock-api.com/assets-api-res/images/heroes/wraith_sm.webp", card: "https://assets-bucket.deadlock-api.com/assets-api-res/images/heroes/wraith_card.webp" },
  { id: 27, name: "Yamato", portrait: "https://assets-bucket.deadlock-api.com/assets-api-res/images/heroes/yamato_sm.webp", card: "https://assets-bucket.deadlock-api.com/assets-api-res/images/heroes/yamato_card.webp" },
]

export function mockHelden(): DraftHero[] {
  return HERO_FIXTURES.map((h) => ({
    id: h.id,
    name: h.name,
    image_url: h.portrait,
    card_image_url: h.card,
  }))
}

function lese(code: string): MockRaum | null {
  const roh = window.localStorage.getItem(SPEICHER_RAUM + code)
  return roh ? (JSON.parse(roh) as MockRaum) : null
}

function schreibe(raum: MockRaum) {
  window.localStorage.setItem(SPEICHER_RAUM + raum.code, JSON.stringify(raum))
}

function neuerCode(): string {
  let code = ''
  do {
    code = Array.from(
      { length: 6 },
      () => CODE_ALPHABET[Math.floor(Math.random() * CODE_ALPHABET.length)],
    ).join('')
  } while (window.localStorage.getItem(SPEICHER_RAUM + code))
  return code
}

function neueSequenz(bansProTeam: number): DraftSequenzSchritt[] {
  const schritte: DraftSequenzSchritt[] = []
  for (let i = 0; i < bansProTeam; i++) {
    schritte.push({ index: schritte.length, team: 1, action: 'ban' })
    schritte.push({ index: schritte.length, team: 2, action: 'ban' })
  }
  const muster: DraftTeamSeite[] = [1, 2, 2, 1, 1, 2]
  for (let runde = 0; runde < 2; runde++) {
    for (const team of muster) {
      schritte.push({ index: schritte.length, team, action: 'pick' })
    }
  }
  return schritte
}

function vergebeneHelden(raum: MockRaum): Set<string> {
  return new Set(raum.actions.map((a) => a.hero_name))
}

function starteNaechstenZug(raum: MockRaum, jetzt: number) {
  if (raum.currentActionIndex >= neueSequenz(raum.bansPerTeam).length) {
    raum.phase = 'abgeschlossen'
    raum.deadlineAt = null
    raum.lobbyStatus = 'angefordert'
    raum.lobbyRequestedAt = jetzt
    raum.lobbyError = null
    return
  }
  raum.deadlineAt =
    raum.roundSeconds > 0 ? new Date(jetzt + raum.roundSeconds * 1000).toISOString() : null
}

function lobbyBereitstellen(raum: MockRaum, jetzt: number) {
  if (raum.lobbyStatus !== 'angefordert' || raum.lobbyRequestedAt === null) return
  if (jetzt - raum.lobbyRequestedAt < LOBBY_DAUER_MS) return
  if (import.meta.env.VITE_DRAFT_MOCK_LOBBY_FAIL === '1') {
    raum.lobbyStatus = 'fehler'
    raum.lobbyError = 'Provisionierung fehlgeschlagen'
    return
  }
  raum.lobbyStatus = 'bereit'
  raum.lobbyJoinCode = Array.from({ length: 6 }, () =>
    CODE_ALPHABET[Math.floor(Math.random() * CODE_ALPHABET.length)],
  ).join('')
}

function autoZug(raum: MockRaum) {
  const sequenz = neueSequenz(raum.bansPerTeam)
  const schritt = sequenz[raum.currentActionIndex]
  if (!schritt) return
  const frei = mockHelden().filter((h) => !vergebeneHelden(raum).has(h.name))
  const held = frei[Math.floor(Math.random() * frei.length)]
  raum.actions.push({
    sequence_index: schritt.index,
    action_type: schritt.action,
    team_slot: schritt.team,
    hero_name: held.name,
    is_auto: true,
  })
  raum.currentActionIndex += 1
  starteNaechstenZug(raum, Date.now())
}

function fortschreiben(code: string, viewer: string | null): MockRaum {
  const vorDemLesen = window.localStorage.getItem(SPEICHER_RAUM + code)
  const raum = lese(code)
  if (!raum) {
    throw new DraftMockFehler(404, 'Diesen Draft gibt es nicht')
  }
  const jetzt = Date.now()
  if (viewer) {
    const roh = window.localStorage.getItem(SPEICHER_ZUSCHAUER)
    const karte = roh ? (JSON.parse(roh) as Record<string, number>) : {}
    karte[viewer] = jetzt
    for (const [id, gesehen] of Object.entries(karte)) {
      if (jetzt - gesehen > ZUSCHAUER_FENSTER_MS * 4) delete karte[id]
    }
    window.localStorage.setItem(SPEICHER_ZUSCHAUER, JSON.stringify(karte))
  }
  const vorIndex = raum.currentActionIndex
  const vorLobby = raum.lobbyStatus
  while (
    raum.phase === 'laeuft' &&
    raum.deadlineAt !== null &&
    Date.now() > new Date(raum.deadlineAt).getTime() &&
    raum.currentActionIndex < neueSequenz(raum.bansPerTeam).length
  ) {
    autoZug(raum)
  }
  lobbyBereitstellen(raum, Date.now())
  if (raum.currentActionIndex !== vorIndex || raum.lobbyStatus !== vorLobby) {
    const aktuell = window.localStorage.getItem(SPEICHER_RAUM + code)
    if (aktuell !== vorDemLesen) return fortschreiben(code, null)
    schreibe(raum)
  }
  return raum
}

function teamVonToken(raum: MockRaum, token: string | null): DraftTeamSeite | null {
  if (!token) return null
  if (raum.team1Token && raum.team1Token === token) return 1
  if (raum.team2Token && raum.team2Token === token) return 2
  throw new DraftMockFehler(403, 'Dieser Captain-Platz gehört dir nicht')
}

function teamVonViewer(raum: MockRaum, viewer: string | null): DraftTeamSeite | null {
  if (!viewer) return null
  if (raum.team1Viewer === viewer) return 1
  if (raum.team2Viewer === viewer) return 2
  return null
}

function zustaendigesTeam(raum: MockRaum, token: string | null, viewer: string | null): DraftTeamSeite {
  const perToken = teamVonToken(raum, token)
  if (perToken) return perToken
  const perViewer = teamVonViewer(raum, viewer)
  if (perViewer) return perViewer
  throw new DraftMockFehler(403, 'Du bist in diesem Raum kein Captain')
}

export function mockAnlegen(body: DraftRaumAnlegen): DraftRaumCode {
  const bans = Math.max(0, Math.min(6, body.bans_per_team ?? 2))
  const sekunden = [0, 30, 45, 60, 90].includes(body.round_seconds ?? 30)
    ? (body.round_seconds ?? 30)
    : 30
  const raum: MockRaum = {
    code: neuerCode(),
    team1Name: body.team1_name?.trim() || 'Team 1',
    team2Name: body.team2_name?.trim() || 'Team 2',
    bansPerTeam: bans,
    roundSeconds: sekunden,
    team1Viewer: null,
    team2Viewer: null,
    team1Token: null,
    team2Token: null,
    team1Ready: false,
    team2Ready: false,
    phase: 'warteraum',
    currentActionIndex: 0,
    deadlineAt: null,
    actions: [],
    lobbyStatus: 'keine',
    lobbyJoinCode: null,
    lobbyError: null,
    lobbyRequestedAt: null,
    rematchOf: null,
  }
  schreibe(raum)
  return { code: raum.code }
}

export function mockZustand(code: string, viewer: string | null, token: string | null): DraftRaumZustand {
  const raum = fortschreiben(code, viewer)
  const jetzt = Date.now()
  const roh = window.localStorage.getItem(SPEICHER_ZUSCHAUER)
  const karte = roh ? (JSON.parse(roh) as Record<string, number>) : {}
  const captainViewer = new Set(
    [raum.team1Viewer, raum.team2Viewer].filter((v): v is string => v !== null),
  )
  const zuschauer = Object.entries(karte).filter(
    ([id, gesehen]) =>
      jetzt - gesehen <= ZUSCHAUER_FENSTER_MS && !captainViewer.has(id),
  ).length
  const meinTeam = teamVonToken(raum, token) ?? teamVonViewer(raum, viewer)
  return {
    code: raum.code,
    phase: raum.phase,
    bans_per_team: raum.bansPerTeam,
    round_seconds: raum.roundSeconds,
    team1: { name: raum.team1Name, claimed: raum.team1Viewer !== null, ready: raum.team1Ready },
    team2: { name: raum.team2Name, claimed: raum.team2Viewer !== null, ready: raum.team2Ready },
    spectators: zuschauer,
    you: { team: meinTeam },
    sequence: neueSequenz(raum.bansPerTeam),
    current_action_index: raum.currentActionIndex,
    deadline_at: raum.deadlineAt,
    actions: raum.actions,
    lobby: {
      status: raum.lobbyStatus as DraftRaumZustand['lobby']['status'],
      join_code: raum.lobbyJoinCode,
      error: raum.lobbyError,
      match_id: null,
      result: null,
    },
    rematch_code: null,
  }
}

export function mockClaim(code: string, team: DraftTeamSeite, viewer: string | null): DraftClaimAntwort {
  const raum = lese(code)
  if (!raum) throw new DraftMockFehler(404, 'Diesen Draft gibt es nicht')
  if (raum.phase !== 'warteraum') throw new DraftMockFehler(409, 'Der Draft läuft bereits')
  if (teamVonViewer(raum, viewer)) throw new DraftMockFehler(409, 'Du hast bereits einen Captain-Platz')
  const feld = team === 1 ? 'team1Viewer' : 'team2Viewer'
  if (raum[feld] !== null) throw new DraftMockFehler(409, 'Dieser Platz ist schon belegt')
  raum[feld] = viewer ?? 'ohne-viewer'
  const tokenFeld = team === 1 ? 'team1Token' : 'team2Token'
  const token = `claim-${Math.random().toString(36).slice(2)}${Math.random().toString(36).slice(2)}`
  raum[tokenFeld] = token
  schreibe(raum)
  return { token }
}

export function mockReady(code: string, token: string | null, viewer: string | null): void {
  const raum = fortschreiben(code, viewer)
  const team = zustaendigesTeam(raum, token, viewer)
  if (raum.phase !== 'warteraum') throw new DraftMockFehler(409, 'Der Draft läuft bereits')
  if (team === 1) raum.team1Ready = true
  else raum.team2Ready = true
  if (raum.team1Ready && raum.team2Ready) {
    raum.phase = 'laeuft'
    raum.currentActionIndex = 0
    starteNaechstenZug(raum, Date.now())
  }
  schreibe(raum)
}

export function mockLeave(code: string, token: string | null, viewer: string | null): void {
  const raum = fortschreiben(code, viewer)
  const team = zustaendigesTeam(raum, token, viewer)
  if (raum.phase !== 'warteraum') throw new DraftMockFehler(409, 'Der Draft läuft bereits')
  if (team === 1) {
    raum.team1Viewer = null
    raum.team1Token = null
    raum.team1Ready = false
  } else {
    raum.team2Viewer = null
    raum.team2Token = null
    raum.team2Ready = false
  }
  schreibe(raum)
}

export function mockAktion(code: string, heroName: string, token: string | null, viewer: string | null): void {
  const raum = fortschreiben(code, viewer)
  const team = zustaendigesTeam(raum, token, viewer)
  const sequenz = neueSequenz(raum.bansPerTeam)
  if (raum.phase !== 'laeuft') throw new DraftMockFehler(409, 'Der Draft läuft gerade nicht')
  const schritt = sequenz[raum.currentActionIndex]
  if (!schritt) throw new DraftMockFehler(409, 'Der Draft ist bereits abgeschlossen')
  if (schritt.team !== team) throw new DraftMockFehler(403, 'Das andere Team ist am Zug')
  if (vergebeneHelden(raum).has(heroName)) throw new DraftMockFehler(409, 'Dieser Held ist bereits vergeben')
  raum.actions.push({
    sequence_index: schritt.index,
    action_type: schritt.action,
    team_slot: schritt.team,
    hero_name: heroName,
    is_auto: false,
  })
  raum.currentActionIndex += 1
  starteNaechstenZug(raum, Date.now())
  schreibe(raum)
}

export function mockRematch(code: string): DraftRaumCode {
  const raum = lese(code)
  if (!raum) throw new DraftMockFehler(404, 'Diesen Draft gibt es nicht')
  if (raum.phase !== 'abgeschlossen') throw new DraftMockFehler(409, 'Der Draft läuft noch')
  const neu: MockRaum = {
    ...raum,
    code: neuerCode(),
    team1Name: raum.team2Name,
    team2Name: raum.team1Name,
    team1Viewer: null,
    team2Viewer: null,
    team1Token: null,
    team2Token: null,
    team1Ready: false,
    team2Ready: false,
    phase: 'warteraum',
    currentActionIndex: 0,
    deadlineAt: null,
    actions: [],
    lobbyStatus: 'keine',
    lobbyJoinCode: null,
    lobbyError: null,
    lobbyRequestedAt: null,
    rematchOf: code,
  }
  schreibe(neu)
  return { code: neu.code }
}

export function mockLobbyRetry(code: string, viewer: string | null): void {
  const raum = fortschreiben(code, viewer)
  if (raum.lobbyStatus !== 'fehler') throw new DraftMockFehler(409, 'Es gibt nichts zu wiederholen')
  raum.lobbyStatus = 'angefordert'
  raum.lobbyRequestedAt = Date.now()
  raum.lobbyError = null
  schreibe(raum)
}
