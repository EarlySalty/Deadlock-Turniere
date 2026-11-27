import test from 'node:test'
import assert from 'node:assert/strict'
import { bracketHasResults, isPugTournament, teamOverview, workflowAction, workflowStage } from '../src/lib/adminWorkflow.ts'

const member = (id) => ({ discord_id: id })
const team = (id, ids) => ({ id, members: ids.map(member) })
const signup = (id, teamId = null) => ({ discord_id: id, team_id: teamId })

const tournament = (status, patch = {}) => ({
  status,
  name: 'Deadlock PUG Turnier #1',
  description: 'Pick-Up Game',
  invite_mode: 'solo_only',
  team_size: 6,
  teams: [team(1, ['a', 'b']), team(2, ['c', 'd', 'e', 'f', 'g', 'h'])],
  signups: [signup('a', 1), signup('b', 1), signup('x'), signup('y')],
  groups: [],
  bracket_matches: [],
  ...patch,
})

const NONE = { hasGroups: false, hasBracket: false }

test('Turnierleitung bekommt in jeder Phase die passende nächste Aktion', () => {
  assert.equal(workflowAction(tournament('draft'), NONE).phase, 'setup')
  assert.equal(workflowAction(tournament('registration'), NONE).phase, 'participants')
  assert.equal(workflowAction(tournament('checkin'), NONE).phase, 'checkin')
  assert.equal(workflowAction(tournament('group_phase'), { hasGroups: true, hasBracket: false }).phase, 'group_phase')
  assert.equal(workflowAction(tournament('bracket'), { hasGroups: false, hasBracket: true }).phase, 'bracket')
  assert.equal(workflowAction(tournament('completed'), NONE).stage, 4)
})

test('Ohne sichtbare Spielphase führt der Ablauf ins Setup statt auf einen ausgeblendeten Reiter', () => {
  assert.equal(workflowAction(tournament('group_phase', { groups: [{ matches: [] }] }), NONE).phase, 'setup')
  assert.equal(workflowAction(tournament('bracket'), NONE).phase, 'setup')
})

test('PUG folgt dem Anmeldemodus des Servers, nicht dem Namen', () => {
  assert.equal(isPugTournament(tournament('registration')), true)
  assert.equal(isPugTournament(tournament('registration', { invite_mode: 'always' })), false)
  assert.equal(isPugTournament(tournament('registration', { name: 'Community Cup', invite_mode: 'solo_only' })), true)
  assert.match(workflowAction(tournament('registration'), NONE).description, /Captains/)
})

test('Team-Statistik zählt nur noch nicht zugeteilte Solo-Spieler', () => {
  const counts = teamOverview(tournament('registration', {
    signups: [signup('a', 1), signup('b', 1), signup('x'), signup('c'), signup('y')],
  }))
  assert.deepEqual(counts, { teams: 2, playersInTeams: 8, solos: 2, incompleteTeams: 1 })
})

test('Status-Fortschritt bleibt vor und nach den Spielen konsistent', () => {
  assert.deepEqual(['draft', 'registration', 'checkin', 'group_phase', 'bracket', 'completed', 'archived'].map(workflowStage), [0, 1, 2, 3, 3, 4, 4])
})

test('Nach dem letzten Bracket-Ergebnis zeigt der Ablauf den Turnierabschluss', () => {
  const played = { status: 'completed', winner_id: 1 }
  const open = { status: 'pending', winner_id: null }
  const BRACKET = { hasGroups: false, hasBracket: true }
  assert.equal(workflowAction(tournament('bracket', { bracket_matches: [played, open] }), BRACKET).phase, 'bracket')
  const finished = workflowAction(tournament('bracket', { bracket_matches: [played, { status: 'cancelled', winner_id: null }] }), BRACKET)
  assert.equal(finished.phase, 'setup')
  assert.equal(finished.title, 'Turnier abschließen')
  assert.equal(bracketHasResults(tournament('bracket', { bracket_matches: [open] })), false)
  assert.equal(bracketHasResults(tournament('bracket', { bracket_matches: [open, played] })), true)
})
