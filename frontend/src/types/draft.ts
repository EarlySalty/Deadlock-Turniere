export type DraftPhase = 'warteraum' | 'laeuft' | 'abgeschlossen'

export type DraftAktionArt = 'ban' | 'pick'

export type DraftLobbyStatus =
  | 'keine'
  | 'angefordert'
  | 'bereit'
  | 'fehler'
  | 'gestartet'
  | 'beendet'

export type DraftTeamSeite = 1 | 2

export interface DraftTeamInfo {
  name: string
  claimed: boolean
  ready: boolean
}

export interface DraftSequenzSchritt {
  index: number
  team: DraftTeamSeite
  action: DraftAktionArt
}

export interface DraftAktion {
  sequence_index: number
  action_type: DraftAktionArt
  team_slot: DraftTeamSeite
  hero_name: string
  is_auto: boolean
}

export interface DraftLobbyInfo {
  status: DraftLobbyStatus
  join_code: string | null
  error: string | null
  match_id: number | null
  result: Record<string, unknown> | null
}

export interface DraftRaumZustand {
  code: string
  phase: DraftPhase
  bans_per_team: number
  round_seconds: number
  team1: DraftTeamInfo
  team2: DraftTeamInfo
  spectators: number
  you: { team: DraftTeamSeite | null }
  sequence: DraftSequenzSchritt[]
  current_action_index: number
  deadline_at: string | null
  actions: DraftAktion[]
  lobby: DraftLobbyInfo
  rematch_code: string | null
}

export interface DraftHero {
  id: number
  name: string
  image_url: string
  card_image_url?: string
}

export interface DraftRaumAnlegen {
  team1_name?: string
  team2_name?: string
  bans_per_team?: number
  round_seconds?: number
}

export interface DraftRaumCode {
  code: string
}

export interface DraftClaimAntwort {
  token: string
}
