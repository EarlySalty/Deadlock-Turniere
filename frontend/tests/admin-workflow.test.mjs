import test from 'node:test'
import assert from 'node:assert/strict'
import { isPugTournament, teamOverview, workflowAction, workflowStage } from '../src/lib/adminWorkflow.ts'

const member = (id) => ({ discord_id: id })
const team = (id, ids) => ({ id, members: ids.map(member) })
const signup = (id, teamId = null) => ({ discord_id: id, team_id: teamId })

const tournament = (status, patch = {}) => ({
  status,
  name: 'Deadlock PUG Turnier #1',
  description: 'Pick-Up Game',
  team_size: 6,
  teams: [team(1, ['a', 'b']), team(2, ['c', 'd', 'e', 'f', 'g', 'h'])],
  signups: [signup('a', 1), signup('b', 1), signup('x'), signup('y')],
  groups: [],
  bracket_matches: [],
  ...patch,
})

test('Turnierleitung bekommt in jeder Phase die passende nächste Aktion', () => {
  assert.equal(workflowAction(tournament('draft')).phase, 'setup')
  assert.equal(workflowAction(tournament('registration')).phase, 'participants')
  assert.equal(workflowAction(tournament('checkin')).phase, 'checkin')
  assert.equal(workflowAction(tournament('group_phase', { groups: [{ matches: [] }] })).phase, 'group_phase')
  assert.equal(workflowAction(tournament('bracket', { bracket_matches: [{}] })).phase, 'bracket')
  assert.equal(workflowAction(tournament('bracket')).phase, 'setup')
  assert.equal(workflowAction(tournament('completed')).stage, 4)
})

test('PUG wird erkannt, normale Cups nicht', () => {
  assert.equal(isPugTournament(tournament('registration')), true)
  assert.equal(isPugTournament(tournament('registration', { name: 'Community Cup', description: null })), false)
  assert.match(workflowAction(tournament('registration')).description, /Captains/)
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
