import type { Tournament, TournamentDetail } from '@/types/tournament'
import type { AdminPhase } from '@/components/admin/adminPhase'
import { isLeague } from './league.ts'

export type WorkflowStage = 0 | 1 | 2 | 3 | 4

export interface WorkflowAction {
  stage: WorkflowStage
  title: string
  description: string
  button: string
  phase: AdminPhase
}

export function workflowStage(status: Tournament['status']): WorkflowStage {
  switch (status) {
    case 'draft': return 0
    case 'registration': return 1
    case 'checkin': return 2
    case 'group_phase':
    case 'bracket': return 3
    case 'completed':
    case 'archived': return 4
  }
}

export function isPugTournament(t: Pick<Tournament, 'invite_mode'>): boolean {
  return t.invite_mode === 'solo_only'
}

export interface PhaseAvailability {
  hasGroups: boolean
  hasBracket: boolean
}

const TERMINAL_MATCH_STATUSES = ['completed', 'forfeit', 'cancelled']

export function bracketFinished(t: Pick<TournamentDetail, 'bracket_matches'>): boolean {
  return t.bracket_matches.length > 0 && t.bracket_matches.every((match) => TERMINAL_MATCH_STATUSES.includes(match.status))
}

export function bracketHasResults(t: Pick<TournamentDetail, 'bracket_matches'>): boolean {
  return t.bracket_matches.some((match) => match.winner_id !== null || TERMINAL_MATCH_STATUSES.includes(match.status))
}

export function workflowAction(t: TournamentDetail, { hasGroups, hasBracket }: PhaseAvailability): WorkflowAction {
  switch (t.status) {
    case 'draft':
      return {
        stage: 0,
        title: 'Anmeldung eröffnen',
        description: 'Prüfe Startzeiten und Regeln. Danach kannst du die Anmeldung im Setup freigeben.',
        button: 'Setup öffnen',
        phase: 'setup',
      }
    case 'registration':
      return {
        stage: 1,
        title: 'Anmeldungen prüfen und Teams bilden',
        description: isPugTournament(t)
          ? 'Die Spieler melden sich solo an. Öffne im Setup den Check-in. Sobald die Spieler eingecheckt sind, bildet ein Admin unter „Check-in“ mit „Teams ausgleichen“ gleich starke Teams.'
          : 'Prüfe die Solo-Anmeldungen, vervollständige die Teams und lege Captains fest. Anschließend geht es zum Check-in.',
        button: isPugTournament(t) ? 'Check-in im Setup öffnen' : 'Spieler und Teams verwalten',
        phase: isPugTournament(t) ? 'setup' : 'participants',
      }
    case 'checkin':
      return {
        stage: 2,
        title: 'Check-in kontrollieren und abschließen',
        description: isPugTournament(t)
          ? 'Warte, bis die Spieler eingecheckt sind, dann bildet ein Admin mit „Teams ausgleichen“ die Teams. Über die Vorschau im Check-in startest du das Turnier.'
          : 'Prüfe fehlende Spieler und unvollständige Teams. Über die Vorschau im Check-in kannst du das Turnier starten.',
        button: 'Zum Check-in',
        phase: 'checkin',
      }
    case 'group_phase':
      return {
        stage: 3,
        title: 'Spiele und Ergebnisse verwalten',
        description: isLeague(t.tournament_mode)
          ? 'Öffne den Spielplan, verwalte die Matches und trage Ergebnisse ein. Abgeschlossen wird die Liga im Setup.'
          : 'Öffne den Spielplan, verwalte die Matches und trage Ergebnisse ein. Der Übergang zum Bracket erfolgt im Setup.',
        button: hasGroups ? 'Zum Spielplan' : 'Setup prüfen',
        phase: hasGroups ? 'group_phase' : 'setup',
      }
    case 'bracket':
      if (bracketFinished(t)) {
        return {
          stage: 3,
          title: 'Turnier abschließen',
          description: 'Alle Bracket-Matches haben ein Ergebnis. Prüfe den Endstand und schließe das Turnier im Setup über „Turnier abschließen“ ab.',
          button: 'Zum Abschluss',
          phase: 'setup',
        }
      }
      return {
        stage: 3,
        title: 'Bracket und Ergebnisse verwalten',
        description: 'Steuere die Matches, prüfe die Ergebnisse und schließe das Turnier danach im Setup ab.',
        button: hasBracket ? 'Zu den Matches' : 'Setup prüfen',
        phase: hasBracket ? 'bracket' : 'setup',
      }
    case 'completed':
    case 'archived':
      return {
        stage: 4,
        title: 'Turnier beendet',
        description: t.status === 'archived'
          ? 'Das Turnier ist archiviert. Ergebnisse und Teams bleiben für die Historie erhalten.'
          : 'Das Turnier ist abgeschlossen. Du kannst Ergebnisse prüfen und es anschließend archivieren.',
        button: 'Zum Setup',
        phase: 'setup',
      }
  }
}

export function teamOverview(t: TournamentDetail) {
  const assigned = new Set(t.teams.flatMap((team) => team.members.map((member) => member.discord_id)))
  return {
    teams: t.teams.length,
    playersInTeams: assigned.size,
    solos: t.signups.filter((signup) => signup.team_id === null && !assigned.has(signup.discord_id)).length,
    incompleteTeams: t.teams.filter((team) => team.members.length < t.team_size).length,
  }
}
