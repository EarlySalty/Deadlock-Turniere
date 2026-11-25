import type { Tournament, TournamentDetail } from '@/types/tournament'
import type { AdminPhase } from '@/components/admin/adminPhase'

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

export function isPugTournament(t: Pick<Tournament, 'name' | 'description'>): boolean {
  return /\bpug\b|pick[- ]?up games?/i.test(`${t.name} ${t.description ?? ''}`)
}

export function workflowAction(t: TournamentDetail): WorkflowAction {
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
          ? 'Die Spieler melden sich solo an. Ordne sie unter „Teilnehmer“ per Drag-and-drop den Teams zu und lege die Captains fest. Anschließend öffnest du im Setup den Check-in.'
          : 'Prüfe die Solo-Anmeldungen, vervollständige die Teams und lege Captains fest. Anschließend geht es zum Check-in.',
        button: 'Spieler und Teams verwalten',
        phase: 'participants',
      }
    case 'checkin':
      return {
        stage: 2,
        title: 'Check-in kontrollieren und abschließen',
        description: 'Prüfe fehlende Spieler und unvollständige Teams. Über die Vorschau im Check-in kannst du das Turnier starten.',
        button: 'Zum Check-in',
        phase: 'checkin',
      }
    case 'group_phase':
      return {
        stage: 3,
        title: 'Spiele und Ergebnisse verwalten',
        description: 'Öffne den Spielplan, verwalte die Matches und trage Ergebnisse ein. Der Übergang zum Bracket erfolgt im Setup.',
        button: 'Zum Spielplan',
        phase: t.groups.length > 0 ? 'group_phase' : 'setup',
      }
    case 'bracket':
      return {
        stage: 3,
        title: 'Bracket und Ergebnisse verwalten',
        description: 'Steuere die Matches, prüfe die Ergebnisse und schließe das Turnier danach im Setup ab.',
        button: t.bracket_matches.length > 0 ? 'Zu den Matches' : 'Setup prüfen',
        phase: t.bracket_matches.length > 0 ? 'bracket' : 'setup',
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
