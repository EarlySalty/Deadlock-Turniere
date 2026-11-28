import { motion } from 'framer-motion'
import { GitBranch, Users2 } from 'lucide-react'
import { useMemo } from 'react'
import Card from '@/components/ui/Card'
import BracketMatch from './BracketMatch'
import { bracketStageLabel } from '@/lib/bracketStage'
import { teamNameById } from '@/lib/teamName'
import type { BracketFormat, BracketMatch as BracketMatchType, BracketMiniGroup, TeamPublic } from '@/types/tournament'

interface Props {
  matches: BracketMatchType[]
  teams: TeamPublic[]
  format: BracketFormat
  miniGroups?: BracketMiniGroup[]
}

interface SlotPlaceholders {
  p1: string | null
  p2: string | null
}
type PlaceholderMap = Map<number, SlotPlaceholders>

function computePlaceholder(
  match: BracketMatchType,
  slot: 1 | 2,
  allMatches: BracketMatchType[],
  miniGroups: BracketMiniGroup[],
): string | null {
  const group = miniGroups.find((entry) => entry.advances_to_match_id === match.id && entry.advances_to_slot === slot)
  if (group) return `Sieger Runde ${group.round} · Slot ${group.position + 1}`
  const sourceMatchId = slot === 1 ? match.source_match1_id : match.source_match2_id
  if (sourceMatchId !== null && sourceMatchId !== undefined) {
    const source = allMatches.find((m) => m.id === sourceMatchId)
    if (source) {
      return `Sieger ${bracketStageLabel(source, allMatches)}`
    }
  }
  const loserSource = allMatches.find(
    (m) => m.loser_to_match_id === match.id && m.loser_to_slot === slot,
  )
  if (loserSource) {
    return `Verlierer ${bracketStageLabel(loserSource, allMatches)}`
  }
  return null
}

function buildPlaceholderMap(allMatches: BracketMatchType[], miniGroups: BracketMiniGroup[]): PlaceholderMap {
  const map: PlaceholderMap = new Map()
  for (const match of allMatches) {
    map.set(match.id, {
      p1: computePlaceholder(match, 1, allMatches, miniGroups),
      p2: computePlaceholder(match, 2, allMatches, miniGroups),
    })
  }
  return map
}

type TreeItem =
  | { kind: 'match'; key: string; position: number; match: BracketMatchType }
  | { kind: 'group'; key: string; position: number; match: BracketMatchType; group: BracketMiniGroup }

function treeItem(match: BracketMatchType, miniGroups: BracketMiniGroup[]): TreeItem {
  const group = match.mini_group_id === null ? undefined : miniGroups.find((entry) => entry.id === match.mini_group_id)
  if (!group) return { kind: 'match', key: `match-${match.id}`, position: match.position, match }
  return { kind: 'group', key: `group-${group.id}`, position: group.position, match, group }
}

function treeItems(matches: BracketMatchType[], miniGroups: BracketMiniGroup[]): TreeItem[] {
  const byKey = new Map<string, TreeItem>()
  for (const match of matches) {
    if (isUnplayedFiller(match)) continue
    const item = treeItem(match, miniGroups)
    if (!byKey.has(item.key)) byKey.set(item.key, item)
  }
  return [...byKey.values()]
}

function itemRound(item: TreeItem): number {
  return item.kind === 'group' ? item.group.round : item.match.round
}

function MiniGroupNode({ group, teams }: { group: BracketMiniGroup; teams: TeamPublic[] }) {
  return (
    <div className="w-[210px] flex-shrink-0 overflow-hidden rounded-lg border border-border bg-card shadow-sm">
      <div className="flex items-center gap-1.5 border-b border-border px-3 py-1.5 text-xs text-muted">
        <Users2 size={12} aria-hidden="true" />
        Jeder gegen jeden · Slot {group.position + 1}
      </div>
      <ul className="divide-y divide-border">
        {group.team_ids.map((teamId) => (
          <li key={teamId} className="min-w-0 truncate px-3 py-2 text-sm text-foreground" title={teamNameById(teams, teamId)}>
            {teamNameById(teams, teamId)}
          </li>
        ))}
      </ul>
    </div>
  )
}

interface BracketColumnsProps {
  items: TreeItem[]
  teams: TeamPublic[]
  allMatches: BracketMatchType[]
  placeholders: PlaceholderMap
}

function BracketColumns({ items, teams, allMatches, placeholders }: BracketColumnsProps) {
  const roundsMap = new Map<number, TreeItem[]>()
  for (const item of items) {
    const round = itemRound(item)
    roundsMap.set(round, [...(roundsMap.get(round) ?? []), item])
  }

  const rounds = Array.from(roundsMap.entries())
    .sort(([a], [b]) => a - b)
    .map(([round, roundItems]) => ({
      round,
      items: roundItems.sort((a, b) => a.position - b.position),
    }))

  const maxRound = Math.max(...rounds.map((r) => r.round))

  return (
    <div className="space-y-2">
    {rounds.length > 1 && (
      <p className="text-xs text-muted sm:hidden">Seitlich wischen, um alle {rounds.length} Runden zu sehen.</p>
    )}
    <div className="overflow-x-auto pb-4">
      <div className="inline-flex gap-0 min-w-max">
        {rounds.map((roundData, roundIndex) => (
          <div key={roundData.round} className="flex flex-col items-center">
            <motion.div
              initial={{ opacity: 0, y: -10 }}
              animate={{ opacity: 1, y: 0 }}
              transition={{ delay: roundIndex * 0.1 }}
              className="mb-4"
            >
              <span className="text-xs font-semibold text-foreground uppercase tracking-wider">
                {bracketStageLabel(roundData.items[0].match, allMatches)}
              </span>
            </motion.div>

            <div className="flex flex-col justify-around flex-1 gap-4 relative">
              {roundData.items.map((item, matchIndex) => {
                const spacingMultiplier = Math.pow(2, roundIndex)
                const topPadding = matchIndex === 0
                  ? (spacingMultiplier - 1) * 32
                  : 0
                const gap = roundIndex > 0
                  ? (spacingMultiplier - 1) * 64
                  : 0

                const ph = placeholders.get(item.match.id)

                return (
                  <div
                    key={item.key}
                    className="flex items-center"
                    style={{
                      marginTop: matchIndex === 0 ? topPadding : gap,
                    }}
                  >
                    {roundIndex > 0 && (
                      <div className="w-6 border-t-2 border-border-strong" />
                    )}

                    {item.kind === 'group' ? (
                      <MiniGroupNode group={item.group} teams={teams} />
                    ) : (
                      <BracketMatch
                        match={item.match}
                        teams={teams}
                        placeholder1={ph?.p1 ?? null}
                        placeholder2={ph?.p2 ?? null}
                      />
                    )}

                    {roundData.round < maxRound && (
                      <div className="w-6 border-t-2 border-border-strong" />
                    )}
                  </div>
                )
              })}
            </div>
          </div>
        ))}
      </div>
    </div>
    </div>
  )
}

function isUnplayedFiller(match: BracketMatchType): boolean {
  return match.status === 'cancelled' && match.team1_id === null && match.team2_id === null
}

export default function BracketView({ matches: allMatches, teams, format, miniGroups = [] }: Props) {
  const placeholders = useMemo(() => buildPlaceholderMap(allMatches, miniGroups), [allMatches, miniGroups])
  const items = treeItems(allMatches, miniGroups)

  if (items.length === 0) {
    return (
      <Card className="p-6 text-center">
        <GitBranch size={32} className="mx-auto text-muted mb-3" />
        <p className="text-muted">Bracket noch nicht generiert</p>
      </Card>
    )
  }

  const winners = items.filter((item) => item.match.bracket_type === 'winners')
  const losers = items.filter((item) => item.match.bracket_type === 'losers')
  const grandFinal = items.filter((item) => item.match.bracket_type === 'grand_final')

  const isDoubleElim = format === 'double_elimination'

  if (!isDoubleElim) {
    return (
      <div className="space-y-4">
        <BracketColumns
          items={winners.length > 0 ? winners : items}
          teams={teams}
          allMatches={allMatches}
          placeholders={placeholders}
        />
      </div>
    )
  }

  return (
    <div className="space-y-8">
      <section>
        <h3 className="text-sm font-semibold text-foreground uppercase tracking-wider mb-3">
          Winner-Bracket
        </h3>
        <BracketColumns
          items={winners}
          teams={teams}
          allMatches={allMatches}
          placeholders={placeholders}
        />
      </section>

      {losers.length > 0 && (
        <section>
          <h3 className="text-sm font-semibold text-foreground uppercase tracking-wider mb-3">
            Loser-Bracket
          </h3>
          <BracketColumns
            items={losers}
            teams={teams}
            allMatches={allMatches}
            placeholders={placeholders}
          />
        </section>
      )}

      {grandFinal.length > 0 && (
        <section>
          <h3 className="text-sm font-semibold text-foreground uppercase tracking-wider mb-3">
            Grand Final
          </h3>
          <BracketColumns
            items={grandFinal}
            teams={teams}
            allMatches={allMatches}
            placeholders={placeholders}
          />
        </section>
      )}
    </div>
  )
}
