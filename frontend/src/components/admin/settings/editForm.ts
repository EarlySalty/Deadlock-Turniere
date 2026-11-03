import { isLeague, localInputToIso, toLocalInput } from '@/lib/league'
import type { Tournament, TournamentMode, TournamentUpdate } from '@/types/tournament'
import { lobbyChanged, lobbyFromStored, resolveLobbyPayload, type GameRulesState } from './gameRules'
import { DATE_HINTS } from './options'
import { DEFAULT_REMINDER_OFFSETS, DEFAULT_START_REMINDER_OFFSETS, offsetsOrDefault } from '../reminderOffsets'
import type { SpecialSettingsState } from './specialSettings'
import type { BasicsValue, DateEntry, MatchFormatValue, ReminderValue } from './StandardFields'
import type { SeriesFormat } from './createFormValues'

export interface EditForm extends BasicsValue, MatchFormatValue, ReminderValue {
  tournamentMode: TournamentMode
  registrationStart: string
  registrationEnd: string
  checkinStart: string
  groupPhaseStart: string
  bracketStart: string
}

export interface EditState {
  form: EditForm
  special: SpecialSettingsState
  gameRules: GameRulesState
}

export type UpdatePayloadResult = { ok: true; payload: TournamentUpdate } | { ok: false; error: string }

function toInputDateTime(value: string | null | undefined): string {
  if (!value) return ''
  return value.replace(' ', 'T').slice(0, 16)
}

export function storedGroupStart(tournament: Tournament): string {
  return isLeague(tournament.tournament_mode)
    ? toLocalInput(tournament.group_phase_start)
    : toInputDateTime(tournament.group_phase_start)
}

export function initialEditState(tournament: Tournament): EditState {
  return {
    form: {
      name: tournament.name,
      description: tournament.description ?? '',
      rules: tournament.rules ?? '',
      teamSize: tournament.team_size,
      seriesFormat: tournament.series_format as SeriesFormat,
      finalSeriesFormat: tournament.final_series_format as SeriesFormat | null,
      bracketFormat: tournament.bracket_format,
      tournamentMode: tournament.tournament_mode,
      registrationStart: toInputDateTime(tournament.registration_start),
      registrationEnd: toInputDateTime(tournament.registration_end),
      checkinStart: toInputDateTime(tournament.checkin_start ?? null),
      groupPhaseStart: storedGroupStart(tournament),
      bracketStart: toInputDateTime(tournament.bracket_start),
      reminderOffsets: offsetsOrDefault(tournament.reminder_offsets ?? [], DEFAULT_REMINDER_OFFSETS),
      startReminderOffsets: offsetsOrDefault(tournament.start_reminder_offsets ?? [], DEFAULT_START_REMINDER_OFFSETS),
    },
    special: {
      inviteMode: tournament.invite_mode,
      inviteWindowStart: toInputDateTime(tournament.invite_window_start),
      inviteWindowEnd: toInputDateTime(tournament.invite_window_end),
      noShowGrace: tournament.no_show_grace_minutes,
      excludeFromLeaderboard: tournament.exclude_from_leaderboard,
    },
    gameRules: {
      gameMode: tournament.tournament_game_mode,
      autoLobby: tournament.auto_lobby_enabled,
      matchObjective: tournament.match_objective,
      ...lobbyFromStored(tournament.lobby_settings),
    },
  }
}

function groupStartPayload(form: EditForm, tournament: Tournament): string | undefined {
  const league = isLeague(form.tournamentMode)
  const storedLeague = isLeague(tournament.tournament_mode)
  if (!league && !storedLeague) return form.groupPhaseStart || undefined
  if (form.groupPhaseStart === storedGroupStart(tournament)) return undefined
  return localInputToIso(form.groupPhaseStart)
}

function windowBoundPayload(windowed: boolean, value: string, stored: string | null | undefined): string | null | undefined {
  if (windowed) return value || null
  return stored ? null : undefined
}

function specialPayload(special: SpecialSettingsState, tournament: Tournament): TournamentUpdate {
  const windowed = special.inviteMode === 'window'
  return {
    invite_mode: special.inviteMode,
    invite_window_start: windowBoundPayload(windowed, special.inviteWindowStart, tournament.invite_window_start),
    invite_window_end: windowBoundPayload(windowed, special.inviteWindowEnd, tournament.invite_window_end),
    no_show_grace_minutes: special.noShowGrace,
    exclude_from_leaderboard: special.excludeFromLeaderboard,
  }
}

export function buildUpdatePayload(
  state: EditState,
  initial: EditState,
  tournament: Tournament,
  canChangeTournamentMode: boolean,
): UpdatePayloadResult {
  const { form, gameRules } = state
  const payload: TournamentUpdate = {
    name: form.name.trim(),
    description: form.description.trim() || undefined,
    team_size: form.teamSize,
    series_format: form.seriesFormat,
    final_series_format: form.finalSeriesFormat ?? null,
    bracket_format: form.bracketFormat,
    tournament_game_mode: gameRules.gameMode,
    auto_lobby_enabled: gameRules.autoLobby,
    match_objective: gameRules.matchObjective,
    registration_start: form.registrationStart || undefined,
    registration_end: form.registrationEnd || undefined,
    checkin_start: form.checkinStart || undefined,
    group_phase_start: groupStartPayload(form, tournament),
    bracket_start: form.bracketStart || undefined,
    reminder_offsets: offsetsOrDefault(form.reminderOffsets, DEFAULT_REMINDER_OFFSETS),
    start_reminder_offsets: offsetsOrDefault(form.startReminderOffsets, DEFAULT_START_REMINDER_OFFSETS),
    rules: form.rules.trim() || null,
    ...specialPayload(state.special, tournament),
  }
  if (canChangeTournamentMode && form.tournamentMode !== tournament.tournament_mode) {
    payload.force_tournament_mode = form.tournamentMode
  }
  if (!lobbyChanged(gameRules, initial.gameRules)) return { ok: true, payload }
  const lobby = resolveLobbyPayload(gameRules)
  if (!lobby.ok) return lobby
  return { ok: true, payload: { ...payload, lobby_settings_preset: lobby.preset, lobby_settings: lobby.settings } }
}

export function editDateEntries(
  form: EditForm,
  update: (patch: Partial<EditForm>) => void,
  league: boolean,
): DateEntry[] {
  const entries: DateEntry[] = [
    { key: 'reg-start', label: 'Anmeldung Start', value: form.registrationStart, onChange: (registrationStart) => update({ registrationStart }) },
    { key: 'reg-end', label: 'Turnier-Start', hint: DATE_HINTS.registrationEnd, value: form.registrationEnd, onChange: (registrationEnd) => update({ registrationEnd }) },
    { key: 'checkin-start', label: 'Check-in Start', hint: DATE_HINTS.checkinStart, value: form.checkinStart, onChange: (checkinStart) => update({ checkinStart }) },
    {
      key: 'group-start',
      label: league ? 'Erste Runde beginnt' : 'Gruppenphase Start',
      hint: league ? 'Verschiebt den ganzen Spielplan, sobald er steht. Die Rundenlänge stellst du unter „Spielplan“ ein.' : undefined,
      value: form.groupPhaseStart,
      onChange: (groupPhaseStart) => update({ groupPhaseStart }),
    },
  ]
  if (league) return entries
  return [
    ...entries,
    { key: 'bracket-start', label: 'Bracket Start', value: form.bracketStart, onChange: (bracketStart) => update({ bracketStart }) },
  ]
}
