import { useMemo, useState, type DragEvent } from 'react'
import { useQueryClient } from '@tanstack/react-query'
import { AlertCircle, Pencil, Plus, Save, Star, Trash2, Undo2, UserMinus, Users } from 'lucide-react'
import Card from '@/components/ui/Card'
import Button from '@/components/ui/Button'
import {
  EMPTY_EDITS,
  buildView,
  captainChanges,
  finalLocations,
  planSave,
  snapshotOf,
  type BuilderEdits,
  type BuilderPlayer,
  type BuilderTeamView,
  type ServerSnapshot,
} from '@/lib/teamBuilder'
import { saveTeamBuilder, type SaveContext } from '@/components/admin/teamBuilderSave'
import type { Team, TournamentSignup } from '@/types/tournament'

interface TeamBuilderPanelProps {
  tournamentId: number
  teamSize: number
  teams: Team[]
  signups: TournamentSignup[]
}

const DRAG_TYPE = 'text/plain'

function formatSignupTime(value: string | null): string {
  if (!value) return 'aus Team'
  return new Date(value).toLocaleString('de-DE', {
    day: '2-digit',
    month: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
  })
}

function matchesFilter(player: BuilderPlayer, filter: string): boolean {
  const needle = filter.trim().toLowerCase()
  if (!needle) return true
  return `${player.name} ${player.rank ?? ''}`.toLowerCase().includes(needle)
}

function validateTeamName(name: string, teams: BuilderTeamView[], ownKey: string | null): string | null {
  const trimmed = name.trim()
  if (trimmed.length < 2 || trimmed.length > 32) return 'Der Teamname braucht 2 bis 32 Zeichen.'
  const taken = teams.some((t) => t.key !== ownKey && t.name.trim().toLowerCase() === trimmed.toLowerCase())
  return taken ? 'Diesen Teamnamen gibt es schon.' : null
}

function withPlacement(
  edits: BuilderEdits,
  snapshot: ServerSnapshot,
  discordId: string,
  teamKey: string | null,
): BuilderEdits {
  const base = finalLocations(snapshot, EMPTY_EDITS).get(discordId)
  const placements = { ...edits.placements }
  if (base === teamKey) delete placements[discordId]
  else placements[discordId] = teamKey
  const captains = Object.fromEntries(
    Object.entries(edits.captains).filter(([key, id]) => id !== discordId || key === teamKey),
  )
  return { ...edits, placements, captains }
}

function withoutNewTeam(edits: BuilderEdits, teamKey: string): BuilderEdits {
  const placements = Object.fromEntries(
    Object.entries(edits.placements).filter(([, key]) => key !== teamKey),
  )
  const names = { ...edits.names }
  delete names[teamKey]
  const captains = { ...edits.captains }
  delete captains[teamKey]
  return { newTeams: edits.newTeams.filter((t) => t.key !== teamKey), names, placements, captains }
}

function withCaptain(edits: BuilderEdits, snapshot: ServerSnapshot, teamKey: string, discordId: string): BuilderEdits {
  const current = snapshot.teams.find((t) => t.key === teamKey)?.captainId ?? null
  const captains = { ...edits.captains }
  if (current === discordId) delete captains[teamKey]
  else captains[teamKey] = discordId
  return { ...edits, captains }
}

function withName(edits: BuilderEdits, snapshot: ServerSnapshot, teamKey: string, name: string): BuilderEdits {
  const trimmed = name.trim()
  const original = snapshot.teams.find((t) => t.key === teamKey)?.name
  const names = { ...edits.names }
  if (original === trimmed) delete names[teamKey]
  else names[teamKey] = trimmed
  return { ...edits, names }
}

function readDrag(event: DragEvent): string | null {
  const id = event.dataTransfer.getData(DRAG_TYPE)
  return id || null
}

interface PlayerChipProps {
  player: BuilderPlayer
  selected: boolean
  onSelect: (discordId: string) => void
  compact?: boolean
}

function PlayerChip({ player, selected, onSelect, compact = false }: PlayerChipProps) {
  return (
    <button
      type="button"
      draggable
      onDragStart={(event) => {
        event.dataTransfer.setData(DRAG_TYPE, player.discordId)
        event.dataTransfer.effectAllowed = 'move'
      }}
      onClick={() => onSelect(player.discordId)}
      aria-pressed={selected}
      className={`flex w-full min-w-0 cursor-grab items-center justify-between gap-2 rounded-lg border px-3 py-2 text-left text-sm transition-colors active:cursor-grabbing ${
        selected
          ? 'border-primary bg-primary/15 ring-2 ring-primary/40'
          : 'border-border bg-background/60 hover:border-border-hover hover:bg-card-hover'
      }`}
    >
      <span className="min-w-0">
        <span className="block truncate font-medium text-foreground">{player.name}</span>
        <span className="block truncate text-xs text-muted">
          {player.rank ?? 'Ohne Rang'}
          {compact ? '' : ` · ${formatSignupTime(player.signedUpAt)}`}
        </span>
      </span>
      {!compact && player.rankScore > 0 && (
        <span className="shrink-0 tabular-nums text-xs text-muted">{player.rankScore}</span>
      )}
    </button>
  )
}

interface DropZoneHandlers {
  onDragOver: (event: DragEvent) => void
  onDrop: (event: DragEvent) => void
}

function dropHandlers(accepts: boolean, onDropId: (discordId: string) => void): DropZoneHandlers {
  return {
    onDragOver: (event) => {
      if (!accepts) return
      event.preventDefault()
      event.dataTransfer.dropEffect = 'move'
    },
    onDrop: (event) => {
      event.preventDefault()
      const id = readDrag(event)
      if (id) onDropId(id)
    },
  }
}

interface TeamCardProps {
  team: BuilderTeamView
  teamSize: number
  selectedId: string | null
  allTeams: BuilderTeamView[]
  onSelect: (discordId: string) => void
  onPlace: (discordId: string, teamKey: string | null) => void
  onCaptain: (teamKey: string, discordId: string) => void
  onRename: (teamKey: string, name: string) => void
  onDiscard: (teamKey: string) => void
}

function TeamHeader({ team, allTeams, onRename, onDiscard }: Pick<TeamCardProps, 'team' | 'allTeams' | 'onRename' | 'onDiscard'>) {
  const [editing, setEditing] = useState(false)
  const [value, setValue] = useState(team.name)
  const problem = editing ? validateTeamName(value, allTeams, team.key) : null

  const commit = () => {
    if (problem) return
    onRename(team.key, value)
    setEditing(false)
  }

  if (editing) {
    return (
      <div className="space-y-1">
        <div className="flex gap-2">
          <input
            autoFocus
            value={value}
            onChange={(event) => setValue(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === 'Enter') commit()
              if (event.key === 'Escape') setEditing(false)
            }}
            aria-label="Teamname"
            className="min-w-0 flex-1 rounded-lg border border-border bg-background px-2 py-1 text-sm text-foreground focus:outline-none focus:ring-2 focus:ring-primary/50"
          />
          <Button variant="secondary" size="sm" disabled={Boolean(problem)} onClick={commit}>
            OK
          </Button>
        </div>
        {problem && <p className="text-xs text-red-400">{problem}</p>}
      </div>
    )
  }

  return (
    <div className="flex items-center justify-between gap-2">
      <h4 className="min-w-0 truncate font-semibold text-foreground">
        {team.name}
        {team.serverId === null && <span className="ml-2 text-xs font-normal text-primary">neu</span>}
      </h4>
      <div className="flex shrink-0 gap-1">
        <button
          type="button"
          onClick={() => {
            setValue(team.name)
            setEditing(true)
          }}
          aria-label={`${team.name} umbenennen`}
          className="rounded p-1 text-muted hover:bg-card-hover hover:text-foreground"
        >
          <Pencil size={14} />
        </button>
        {team.serverId === null && (
          <button
            type="button"
            onClick={() => onDiscard(team.key)}
            aria-label={`${team.name} verwerfen`}
            className="rounded p-1 text-muted hover:bg-red-500/10 hover:text-red-300"
          >
            <Trash2 size={14} />
          </button>
        )}
      </div>
    </div>
  )
}

function MemberSlot({
  team,
  player,
  selected,
  onSelect,
  onPlace,
  onCaptain,
}: Pick<TeamCardProps, 'team' | 'onSelect' | 'onPlace' | 'onCaptain'> & { player: BuilderPlayer; selected: boolean }) {
  const isCaptain = team.captainId === player.discordId
  return (
    <li className="flex items-center gap-1">
      <button
        type="button"
        onClick={() => onCaptain(team.key, player.discordId)}
        aria-label={isCaptain ? `${player.name} ist Captain` : `${player.name} zum Captain machen`}
        aria-pressed={isCaptain}
        title={isCaptain ? 'Captain' : 'Zum Captain machen'}
        className={`shrink-0 rounded p-1.5 ${isCaptain ? 'text-primary' : 'text-muted hover:text-foreground'}`}
      >
        <Star size={15} fill={isCaptain ? 'currentColor' : 'none'} />
      </button>
      <div className="min-w-0 flex-1">
        <PlayerChip player={player} selected={selected} onSelect={onSelect} compact />
      </div>
      <button
        type="button"
        onClick={() => onPlace(player.discordId, null)}
        aria-label={`${player.name} zurück in die Liste`}
        title="Zurück in die Liste"
        className="shrink-0 rounded p-1.5 text-muted hover:bg-card-hover hover:text-foreground"
      >
        <UserMinus size={15} />
      </button>
    </li>
  )
}

function TeamCard(props: TeamCardProps) {
  const { team, teamSize, selectedId, onSelect, onPlace } = props
  const free = Math.max(teamSize - team.members.length, 0)
  const selectedOutside = selectedId !== null && !team.members.some((m) => m.discordId === selectedId)
  const handlers = dropHandlers(free > 0, (id) => onPlace(id, team.key))

  return (
    <div {...handlers} className="space-y-3 rounded-xl border border-border bg-card/60 p-3">
      <TeamHeader {...props} />
      <p className="text-xs text-muted">
        {team.members.length}/{teamSize} Plätze belegt
      </p>
      <ul className="space-y-1.5">
        {team.members.map((player) => (
          <MemberSlot
            key={player.discordId}
            team={team}
            player={player}
            selected={selectedId === player.discordId}
            onSelect={onSelect}
            onPlace={onPlace}
            onCaptain={props.onCaptain}
          />
        ))}
        {Array.from({ length: free }, (_, index) => (
          <li key={`free-${index}`}>
            <button
              type="button"
              disabled={!selectedOutside}
              onClick={() => selectedId && onPlace(selectedId, team.key)}
              className={`w-full rounded-lg border border-dashed px-3 py-2 text-left text-xs transition-colors ${
                selectedOutside
                  ? 'border-primary/60 text-primary hover:bg-primary/10'
                  : 'border-border text-muted'
              }`}
            >
              {selectedOutside ? 'Hier einsetzen' : 'Freier Platz'}
            </button>
          </li>
        ))}
      </ul>
    </div>
  )
}

interface PoolProps {
  pool: BuilderPlayer[]
  selectedId: string | null
  onSelect: (discordId: string) => void
  onPlace: (discordId: string, teamKey: string | null) => void
}

function PoolColumn({ pool, selectedId, onSelect, onPlace }: PoolProps) {
  const [filter, setFilter] = useState('')
  const visible = pool.filter((player) => matchesFilter(player, filter))
  const selectedInTeam = selectedId !== null && !pool.some((p) => p.discordId === selectedId)
  const handlers = dropHandlers(true, (id) => onPlace(id, null))

  return (
    <div {...handlers} className="space-y-3 rounded-xl border border-border bg-background/40 p-3">
      <div className="flex items-baseline justify-between gap-2">
        <h4 className="font-semibold text-foreground">Ohne Team</h4>
        <span className="text-xs text-muted">{pool.length} Spieler</span>
      </div>
      <input
        type="search"
        value={filter}
        onChange={(event) => setFilter(event.target.value)}
        placeholder="Name oder Rang suchen"
        aria-label="Spieler filtern"
        className="w-full rounded-lg border border-border bg-background px-3 py-2 text-sm text-foreground focus:outline-none focus:ring-2 focus:ring-primary/50"
      />
      {selectedInTeam && (
        <Button variant="secondary" size="sm" className="w-full" onClick={() => onPlace(selectedId, null)}>
          <Undo2 size={14} />
          Zurück in die Liste
        </Button>
      )}
      {visible.length === 0 ? (
        <p className="text-sm text-muted">
          {pool.length === 0 ? 'Alle Solo-Anmeldungen sind einem Team zugeteilt.' : 'Kein Treffer.'}
        </p>
      ) : (
        <ul className="max-h-[32rem] space-y-1.5 overflow-y-auto pr-1">
          {visible.map((player) => (
            <li key={player.discordId}>
              <PlayerChip player={player} selected={selectedId === player.discordId} onSelect={onSelect} />
            </li>
          ))}
        </ul>
      )}
    </div>
  )
}

function NewTeamForm({ teams, onCreate }: { teams: BuilderTeamView[]; onCreate: (name: string) => void }) {
  const [name, setName] = useState('')
  const problem = name.trim() ? validateTeamName(name, teams, null) : null

  const submit = () => {
    if (!name.trim() || problem) return
    onCreate(name.trim())
    setName('')
  }

  return (
    <div className="space-y-1">
      <div className="flex flex-col gap-2 sm:flex-row">
        <input
          value={name}
          onChange={(event) => setName(event.target.value)}
          onKeyDown={(event) => event.key === 'Enter' && submit()}
          placeholder="Name für ein neues Team"
          aria-label="Name für ein neues Team"
          className="min-w-0 flex-1 rounded-lg border border-border bg-background px-3 py-2 text-sm text-foreground focus:outline-none focus:ring-2 focus:ring-primary/50"
        />
        <Button variant="secondary" size="sm" disabled={!name.trim() || Boolean(problem)} onClick={submit}>
          <Plus size={14} />
          Team anlegen
        </Button>
      </div>
      {problem && <p className="text-xs text-red-400">{problem}</p>}
    </div>
  )
}

function saveContext(tournamentId: number, snapshot: ServerSnapshot, teams: BuilderTeamView[], players: BuilderPlayer[]): SaveContext {
  return {
    tournamentId,
    teamIds: new Map(snapshot.teams.map((t) => [t.key, t.id])),
    teamNames: new Map(teams.map((t) => [t.key, t.name])),
    playerNames: new Map(players.map((p) => [p.discordId, p.name])),
  }
}

export default function TeamBuilderPanel({ tournamentId, teamSize, teams, signups }: TeamBuilderPanelProps) {
  const queryClient = useQueryClient()
  const [edits, setEdits] = useState<BuilderEdits>(EMPTY_EDITS)
  const [selectedId, setSelectedId] = useState<string | null>(null)
  const [progress, setProgress] = useState<{ done: number; total: number } | null>(null)
  const [notice, setNotice] = useState<{ tone: 'ok' | 'error'; text: string } | null>(null)

  const snapshot = useMemo(() => snapshotOf(teams, signups), [teams, signups])
  const view = useMemo(() => buildView(teams, signups, edits), [teams, signups, edits])
  const steps = useMemo(() => planSave(snapshot, edits, teamSize), [snapshot, edits, teamSize])
  const captains = useMemo(() => captainChanges(view, snapshot, edits), [view, snapshot, edits])
  const changeCount = steps.length + captains.length
  const saving = progress !== null

  const place = (discordId: string, teamKey: string | null) => {
    const target = view.teams.find((t) => t.key === teamKey)
    const alreadyThere = target?.members.some((m) => m.discordId === discordId) ?? false
    if (target && !alreadyThere && target.members.length >= teamSize) {
      setNotice({ tone: 'error', text: `„${target.name}“ ist schon voll.` })
      return
    }
    setEdits((current) => withPlacement(current, snapshot, discordId, teamKey))
    setSelectedId(null)
    setNotice(null)
  }

  const select = (discordId: string) => setSelectedId((current) => (current === discordId ? null : discordId))

  const createTeam = (name: string) =>
    setEdits((current) => ({ ...current, newTeams: [...current.newTeams, { key: `n${Date.now()}`, name }] }))

  const rename = (teamKey: string, name: string) => {
    const isNew = edits.newTeams.some((t) => t.key === teamKey)
    setEdits((current) =>
      isNew
        ? { ...current, newTeams: current.newTeams.map((t) => (t.key === teamKey ? { ...t, name: name.trim() } : t)) }
        : withName(current, snapshot, teamKey, name),
    )
  }

  const save = async () => {
    const players = [...view.pool, ...view.teams.flatMap((t) => t.members)]
    const ctx = saveContext(tournamentId, snapshot, view.teams, players)
    const wishes = view.teams
      .filter((t) => edits.captains[t.key] && t.captainId === edits.captains[t.key])
      .map((t) => ({ teamKey: t.key, discordId: edits.captains[t.key] }))
    setNotice(null)
    try {
      await saveTeamBuilder(ctx, steps, wishes, (done, total) => setProgress({ done, total }))
      setNotice({ tone: 'ok', text: 'Teams gespeichert.' })
    } catch (error) {
      setNotice({ tone: 'error', text: error instanceof Error ? error.message : 'Speichern fehlgeschlagen.' })
    } finally {
      await queryClient.invalidateQueries({ queryKey: ['admin', 'tournaments'] })
      await queryClient.invalidateQueries({ queryKey: ['tournaments'] })
      setEdits(EMPTY_EDITS)
      setSelectedId(null)
      setProgress(null)
    }
  }

  const reset = () => {
    setEdits(EMPTY_EDITS)
    setSelectedId(null)
    setNotice(null)
  }

  return (
    <Card className="space-y-4 p-6">
      <div className="flex items-start gap-2">
        <Users size={18} className="mt-1 shrink-0 text-primary" />
        <div>
          <h2 className="text-lg font-semibold text-foreground">Teams von Hand bilden</h2>
          <p className="mt-1 text-sm text-muted">
            Spieler aus der Liste auf einen freien Platz ziehen. Ohne Maus: Spieler antippen, dann den Platz.
            Der Stern macht jemanden zum Captain. Erst „Speichern“ überträgt alles.
          </p>
        </div>
      </div>

      <NewTeamForm teams={view.teams} onCreate={createTeam} />

      <div className="grid gap-4 md:grid-cols-[minmax(0,17rem)_minmax(0,1fr)]">
        <PoolColumn pool={view.pool} selectedId={selectedId} onSelect={select} onPlace={place} />
        {view.teams.length === 0 ? (
          <p className="rounded-xl border border-dashed border-border p-4 text-sm text-muted">
            Noch kein Team. Lege oben eins an.
          </p>
        ) : (
          <div className="grid content-start gap-3 sm:grid-cols-2">
            {view.teams.map((team) => (
              <TeamCard
                key={team.key}
                team={team}
                teamSize={teamSize}
                selectedId={selectedId}
                allTeams={view.teams}
                onSelect={select}
                onPlace={place}
                onCaptain={(key, id) => setEdits((current) => withCaptain(current, snapshot, key, id))}
                onRename={rename}
                onDiscard={(key) => setEdits((current) => withoutNewTeam(current, key))}
              />
            ))}
          </div>
        )}
      </div>

      {notice && (
        <div
          role="status"
          className={`flex items-start gap-2 rounded-lg border p-3 text-sm ${
            notice.tone === 'ok'
              ? 'border-green-500/20 bg-green-500/10 text-green-400'
              : 'border-red-500/20 bg-red-500/10 text-red-400'
          }`}
        >
          {notice.tone === 'error' && <AlertCircle size={16} className="mt-0.5 shrink-0" />}
          <span>{notice.text}</span>
        </div>
      )}

      <div className="flex flex-wrap items-center justify-end gap-2 border-t border-border pt-4">
        {progress && (
          <span className="mr-auto text-sm text-muted">
            Speichere {Math.min(progress.done + 1, progress.total)} von {progress.total}
          </span>
        )}
        {!progress && changeCount > 0 && (
          <span className="mr-auto text-sm text-muted">
            {changeCount === 1 ? '1 Änderung offen' : `${changeCount} Änderungen offen`}
          </span>
        )}
        <Button variant="ghost" size="sm" disabled={saving || changeCount === 0} onClick={reset}>
          <Undo2 size={14} />
          Verwerfen
        </Button>
        <Button variant="primary" size="sm" disabled={saving || changeCount === 0} onClick={() => void save()}>
          <Save size={14} />
          {saving ? 'Speichert …' : 'Speichern'}
        </Button>
      </div>
    </Card>
  )
}
