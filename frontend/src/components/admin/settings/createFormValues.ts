import type { BracketFormat, TournamentDetail } from '@/types/tournament'
import { defaultGameRules, lobbyFromStored, type GameRulesState } from './gameRules'
import type { TournamentFormat } from './options'
import { DEFAULT_REMINDER_OFFSETS, DEFAULT_START_REMINDER_OFFSETS, offsetsOrDefault } from '../reminderOffsets'
import { defaultSpecialSettings, type SpecialSettingsState } from './specialSettings'

export type SeriesFormat = 1 | 3 | 5

export interface CreateFormValues {
  name: string
  description: string
  rules: string
  teamSize: number
  tournamentFormat: TournamentFormat
  bracketFormat: BracketFormat
  seriesFormat: SeriesFormat
  finalSeriesFormat: SeriesFormat | null
  reminderOffsets: number[]
  startReminderOffsets: number[]
  special: SpecialSettingsState
  gameRules: GameRulesState
}

export interface CreateTemplate {
  sourceName: string
  values: CreateFormValues
}

export function defaultCreateValues(): CreateFormValues {
  return {
    name: '',
    description: '',
    rules: '',
    teamSize: 6,
    tournamentFormat: 'auto',
    bracketFormat: 'single_elimination',
    seriesFormat: 1,
    finalSeriesFormat: null,
    reminderOffsets: DEFAULT_REMINDER_OFFSETS,
    startReminderOffsets: DEFAULT_START_REMINDER_OFFSETS,
    special: defaultSpecialSettings(),
    gameRules: defaultGameRules(),
  }
}

export function nextEditionName(name: string): string {
  const trimmed = name.trim()
  const match = /^(.*?)[\s#]*(\d+)$/.exec(trimmed)
  if (!match) return trimmed
  const next = Number(match[2]) + 1
  return match[1] ? `${match[1]} #${next}` : String(next)
}

function seriesOf(value: number | null): SeriesFormat | null {
  return value === 1 || value === 3 || value === 5 ? value : null
}

export function templateFromTournament(tournament: TournamentDetail): CreateTemplate {
  return {
    sourceName: tournament.name,
    values: {
      name: nextEditionName(tournament.name),
      description: tournament.description ?? '',
      rules: tournament.rules ?? '',
      teamSize: tournament.team_size,
      tournamentFormat: tournament.tournament_mode,
      bracketFormat: tournament.bracket_format,
      seriesFormat: seriesOf(tournament.series_format) ?? 1,
      finalSeriesFormat: seriesOf(tournament.final_series_format),
      reminderOffsets: offsetsOrDefault(tournament.reminder_offsets ?? [], DEFAULT_REMINDER_OFFSETS),
      startReminderOffsets: offsetsOrDefault(tournament.start_reminder_offsets ?? [], DEFAULT_START_REMINDER_OFFSETS),
      special: {
        inviteMode: tournament.invite_mode,
        inviteWindowStart: '',
        inviteWindowEnd: '',
        noShowGrace: tournament.no_show_grace_minutes,
        excludeFromLeaderboard: tournament.exclude_from_leaderboard,
      },
      gameRules: {
        gameMode: tournament.tournament_game_mode,
        autoLobby: tournament.auto_lobby_enabled,
        matchObjective: tournament.match_objective,
        ...lobbyFromStored(tournament.lobby_settings),
      },
    },
  }
}
