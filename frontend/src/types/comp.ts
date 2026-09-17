export type CompPriority = 0 | 1 | 2

export interface CompPreference {
  hero_name: string
  priority: CompPriority
}

export interface CompMember {
  id: string
  name: string
  preferences: CompPreference[]
  revision: number
}

export interface CompComposition {
  score: number
  top_priority_count: number
  assignments: { player_index: number; hero_name: string; priority: CompPriority }[]
}

export interface CompRoom {
  code: string
  host_member_id: string
  revision: number
  expires_at: string
  members: CompMember[]
  you: string | null
  unavailable_heroes: string[]
  results: {
    compositions: CompComposition[]
    waiting_for: number[]
    conflict: { player_indices: number[]; available_heroes: string[] } | null
    max_score: number
  }
}
