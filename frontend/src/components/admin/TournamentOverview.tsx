import { CalendarDays, ChevronDown, Plus, Trophy, Users } from 'lucide-react'
import Card from '@/components/ui/Card'
import Badge from '@/components/ui/Badge'
import Button from '@/components/ui/Button'
import { tournamentStart } from '@/lib/adminTournaments'
import type { AdminTournamentSummary } from '@/types/tournament'

function formatDate(value: string | null): string {
  if (!value) return 'offen'
  return new Date(value).toLocaleString('de-DE', {
    weekday: 'short',
    day: '2-digit',
    month: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
  })
}

function signupLine(t: AdminTournamentSummary): string {
  if (t.team_size <= 1) return `${t.signup_count} Anmeldungen`
  return `${t.team_count} Teams · ${t.open_signup_count} Solo offen`
}

interface CreateEntryProps {
  active: boolean
  onCreate: () => void
}

function CreateEntry({ active, onCreate }: CreateEntryProps) {
  return (
    <div className="space-y-2">
      <Button variant={active ? 'secondary' : 'primary'} size="sm" className="w-full" onClick={onCreate}>
        <Plus size={14} />
        Turnier erstellen
      </Button>
      <p className="text-xs text-muted">Name, Termine und Format festlegen.</p>
    </div>
  )
}

interface SidebarProps {
  tournaments: AdminTournamentSummary[]
  selectedId: number | null
  creating: boolean
  onSelect: (id: number) => void
  onCreate: () => void
}

export function TournamentSidebar({ tournaments, selectedId, creating, onSelect, onCreate }: SidebarProps) {
  return (
    <Card className="space-y-4 p-4">
      <CreateEntry active={creating} onCreate={onCreate} />
      <div className="space-y-2 border-t border-border pt-4">
        <h2 className="text-xs font-semibold uppercase tracking-wider text-muted">
          Geplant und laufend ({tournaments.length})
        </h2>
        {tournaments.length === 0 ? (
          <p className="text-xs text-muted">Noch nichts geplant.</p>
        ) : (
          <ul className="space-y-1.5">
            {tournaments.map((t) => (
              <li key={t.id}>
                <button
                  type="button"
                  onClick={() => onSelect(t.id)}
                  aria-current={!creating && selectedId === t.id}
                  className={`w-full rounded-lg border px-3 py-2 text-left transition-colors ${
                    !creating && selectedId === t.id
                      ? 'border-primary/60 bg-primary/10'
                      : 'border-border hover:bg-card-hover'
                  }`}
                >
                  <span className="block truncate text-sm font-medium text-foreground">{t.name}</span>
                  <span className="mt-1 flex items-center gap-2">
                    <Badge status={t.status} mode={t.tournament_mode} />
                    <span className="truncate text-[11px] text-muted">Start {formatDate(tournamentStart(t))}</span>
                  </span>
                </button>
              </li>
            ))}
          </ul>
        )}
      </div>
    </Card>
  )
}

function Fact({ label, value }: { label: string; value: string }) {
  return (
    <div className="min-w-0">
      <dt className="text-[11px] uppercase tracking-wider text-muted">{label}</dt>
      <dd className="truncate text-sm text-foreground">{value}</dd>
    </div>
  )
}

export function TournamentSummaryCard({ tournament: t }: { tournament: AdminTournamentSummary }) {
  return (
    <Card className="space-y-4 p-5">
      <div className="flex flex-wrap items-center gap-3">
        <Trophy size={20} className="shrink-0 text-primary" />
        <h2 className="min-w-0 flex-1 break-words text-xl font-bold text-foreground">{t.name}</h2>
        <Badge status={t.status} mode={t.tournament_mode} />
      </div>
      <dl className="grid grid-cols-2 gap-x-6 gap-y-3 sm:grid-cols-4">
        <Fact label="Anmeldung bis" value={formatDate(t.registration_end)} />
        <Fact label="Check-in ab" value={formatDate(t.checkin_start)} />
        <Fact label="Start" value={formatDate(tournamentStart(t))} />
        <Fact label="Teamgröße" value={t.team_size <= 1 ? 'Einzelspieler' : `${t.team_size} Spieler`} />
        <Fact label="Teams" value={String(t.team_count)} />
        <Fact label="Spieler in Teams" value={String(t.player_count)} />
        <Fact label="Solo-Anmeldungen" value={`${t.signup_count} (${t.open_signup_count} ohne Team)`} />
      </dl>
    </Card>
  )
}

export function CollapsedTournamentRow({ tournament: t, onSelect }: { tournament: AdminTournamentSummary; onSelect: (id: number) => void }) {
  return (
    <button
      type="button"
      onClick={() => onSelect(t.id)}
      aria-expanded={false}
      className="flex w-full flex-wrap items-center gap-x-4 gap-y-1 rounded-xl border border-border bg-card/60 px-4 py-3 text-left transition-colors hover:border-border-hover hover:bg-card-hover"
    >
      <span className="min-w-0 basis-full truncate font-medium text-foreground sm:basis-auto sm:flex-1">{t.name}</span>
      <Badge status={t.status} mode={t.tournament_mode} />
      <span className="flex items-center gap-1 text-xs text-muted">
        <CalendarDays size={12} />
        Start {formatDate(tournamentStart(t))}
      </span>
      <span className="flex items-center gap-1 text-xs text-muted">
        <Users size={12} />
        {signupLine(t)}
      </span>
      <ChevronDown size={16} className="text-muted" />
    </button>
  )
}
