import { useState } from 'react'
import type { TournamentDetail } from '@/types/tournament'
import Card from '@/components/ui/Card'
import GroupStandings from '@/components/groups/GroupStandings'
import BracketView from '@/components/bracket/BracketView'
import MiniGroupPanel from '@/components/bracket/MiniGroupPanel'
import TournamentCasterPanel from '@/components/admin/TournamentCasterPanel'
import { Archive, Trophy, Users2, ListChecks } from 'lucide-react'

type ArchiveTab = 'overview' | 'bracket' | 'groups' | 'teams' | 'caster'

interface ArchivedTournamentViewProps {
  tournament: TournamentDetail
}

const TABS: { key: ArchiveTab; label: string; icon: typeof Trophy }[] = [
  { key: 'overview', label: 'Übersicht', icon: ListChecks },
  { key: 'bracket', label: 'Bracket', icon: Trophy },
  { key: 'groups', label: 'Gruppen', icon: ListChecks },
  { key: 'teams', label: 'Teams', icon: Users2 },
  { key: 'caster', label: 'Caster', icon: Users2 },
]

export default function ArchivedTournamentView({ tournament }: ArchivedTournamentViewProps) {
  const [active, setActive] = useState<ArchiveTab>('overview')

  const visibleTabs = TABS.filter((tab) => {
    if (tab.key === 'bracket') return tournament.bracket_matches.length > 0
    if (tab.key === 'groups') return tournament.groups.some((g) => g.matches.length > 0)
    return true
  })

  const teamCount = tournament.teams.length
  const playerCount = tournament.teams.reduce((sum, t) => sum + t.members.length, 0)
  const matchCount =
    tournament.bracket_matches.length +
    tournament.groups.reduce((sum, g) => sum + g.matches.length, 0)
  const completedMatches = [
    ...tournament.bracket_matches,
    ...tournament.groups.flatMap((g) => g.matches),
  ].filter((m) => m.status === 'completed').length

  return (
    <div className="space-y-4">
      <Card className="border-amber-500/30 bg-amber-500/5 p-4">
        <div className="flex items-start gap-3">
          <Archive size={20} className="mt-0.5 flex-shrink-0 text-amber-400" />
          <div className="flex-1">
            <h2 className="flex flex-wrap items-center gap-2 text-lg font-semibold text-foreground">
              {tournament.name}
              <span className="rounded-full border border-amber-500/40 bg-amber-500/10 px-2 py-0.5 text-xs uppercase text-amber-300">
                Archiv · Read-Only
              </span>
              {tournament.is_test && (
                <span className="rounded-full border border-blue-400/40 bg-blue-500/10 px-2 py-0.5 text-xs uppercase text-blue-300">
                  Test
                </span>
              )}
            </h2>
            <p className="mt-1 text-xs text-muted">
              Status: {tournament.status} · Game-Mode: {tournament.tournament_game_mode}
            </p>
          </div>
        </div>
      </Card>

      <div className="grid grid-cols-2 gap-2 sm:grid-cols-4">
        {[
          { label: 'Teams', value: teamCount },
          { label: 'Spieler', value: playerCount },
          { label: 'Matches', value: matchCount },
          { label: 'Abgeschlossen', value: completedMatches },
        ].map((stat) => (
          <Card key={stat.label} className="p-3 text-center">
            <div className="text-xl font-bold text-foreground">{stat.value}</div>
            <div className="text-xs text-muted">{stat.label}</div>
          </Card>
        ))}
      </div>

      <div role="tablist" className="flex flex-wrap border-b border-border">
        {visibleTabs.map((tab) => (
          <button
            key={tab.key}
            role="tab"
            aria-selected={active === tab.key}
            onClick={() => setActive(tab.key)}
            className={`border-b-2 px-4 py-2.5 text-sm font-medium transition-colors ${
              active === tab.key
                ? 'border-primary text-primary'
                : 'border-transparent text-muted hover:text-foreground'
            }`}
          >
            {tab.label}
          </button>
        ))}
      </div>

      {active === 'overview' && (
        <Card className="space-y-3 p-5">
          <h3 className="font-semibold text-foreground">{tournament.name}</h3>
          {tournament.description && (
            <p className="text-sm text-muted">{tournament.description}</p>
          )}
          <dl className="grid gap-3 text-sm sm:grid-cols-2">
            <div>
              <dt className="text-xs uppercase text-muted">Format</dt>
              <dd>{tournament.bracket_format}</dd>
            </div>
            <div>
              <dt className="text-xs uppercase text-muted">Modus</dt>
              <dd>{tournament.tournament_mode}</dd>
            </div>
            <div>
              <dt className="text-xs uppercase text-muted">Team-Größe</dt>
              <dd>{tournament.team_size}</dd>
            </div>
            <div>
              <dt className="text-xs uppercase text-muted">Game-Mode</dt>
              <dd>{tournament.tournament_game_mode}</dd>
            </div>
          </dl>
        </Card>
      )}

      {active === 'bracket' && (
        <div className="space-y-4">
          {tournament.mini_groups.length > 0 && (
            <MiniGroupPanel
              miniGroups={tournament.mini_groups}
              matches={tournament.bracket_matches}
              teams={tournament.teams}
            />
          )}
          <BracketView
            matches={tournament.bracket_matches}
            teams={tournament.teams}
          />
        </div>
      )}

      {active === 'groups' && (
        <GroupStandings groups={tournament.groups} teams={tournament.teams} />
      )}

      {active === 'teams' && (
        <Card className="p-5">
          <div className="space-y-4">
            {tournament.teams.map((team) => (
              <div
                key={team.id}
                className="rounded-lg border border-border bg-background/40 p-4"
              >
                <div className="mb-2 flex items-center justify-between">
                  <h4 className="font-semibold text-foreground">{team.name}</h4>
                  <span className="text-xs text-muted">
                    {team.members.length} Mitglied(er)
                  </span>
                </div>
                <ul className="space-y-1 text-sm">
                  {team.members.map((m) => (
                    <li
                      key={m.discord_id}
                      className="flex items-center justify-between text-muted"
                    >
                      <span>
                        {m.role === 'captain' && (
                          <span className="mr-1 text-amber-400">★</span>
                        )}
                        {m.discord_name ?? m.discord_id}
                      </span>
                      <span className="text-xs">{m.rank ?? '—'}</span>
                    </li>
                  ))}
                </ul>
              </div>
            ))}
          </div>
        </Card>
      )}

      {active === 'caster' && (
        <TournamentCasterPanel tournamentId={tournament.id} readOnly />
      )}
    </div>
  )
}
