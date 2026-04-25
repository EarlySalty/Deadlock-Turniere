import { Archive, Plus, Trash2, Trophy } from 'lucide-react'
import Card from '@/components/ui/Card'
import Badge from '@/components/ui/Badge'
import Button from '@/components/ui/Button'
import type { Tournament } from '@/types/tournament'

interface AdminTournamentSidebarProps {
  activeTournament: Tournament | null
  archivedTournaments: Tournament[]
  selectedTournamentId: number | null
  onSelectTournament: (id: number) => void
  onCreateClick: () => void
  onDeleteArchived: (id: number, name: string) => void
  deleteDisabled?: boolean
}

export default function AdminTournamentSidebar({
  activeTournament,
  archivedTournaments,
  selectedTournamentId,
  onSelectTournament,
  onCreateClick,
  onDeleteArchived,
  deleteDisabled = false,
}: AdminTournamentSidebarProps) {
  return (
    <aside className="space-y-4">
      <Card className="p-4">
        <div className="mb-3 flex items-center justify-between">
          <h2 className="text-sm font-semibold uppercase tracking-wider text-muted">
            Aktiv
          </h2>
          <Button variant="primary" size="sm" onClick={onCreateClick}>
            <Plus size={14} />
            Neues Turnier
          </Button>
        </div>

        {activeTournament ? (
          <button
            type="button"
            onClick={() => onSelectTournament(activeTournament.id)}
            className={`w-full rounded-lg border p-3 text-left transition-colors ${
              selectedTournamentId === activeTournament.id
                ? 'border-primary/60 bg-primary/10'
                : 'border-border hover:bg-card-hover'
            }`}
          >
            <div className="flex items-center justify-between gap-2">
              <span className="flex items-center gap-2 truncate font-medium text-foreground">
                <Trophy size={14} className="shrink-0 text-primary" />
                <span className="truncate">{activeTournament.name}</span>
              </span>
              <span className="inline-block h-2 w-2 shrink-0 rounded-full bg-green-400" />
            </div>
            <div className="mt-2 flex items-center gap-2">
              <Badge status={activeTournament.status} />
              <span className="text-[10px] text-muted">
                {new Date(activeTournament.created_at).toLocaleDateString('de-DE')}
              </span>
            </div>
          </button>
        ) : (
          <p className="rounded-lg border border-dashed border-border px-3 py-4 text-center text-xs text-muted">
            Kein aktives Turnier
          </p>
        )}
      </Card>

      <Card className="p-4">
        <h2 className="mb-3 flex items-center gap-2 text-sm font-semibold uppercase tracking-wider text-muted">
          <Archive size={14} />
          Archiv ({archivedTournaments.length})
        </h2>

        {archivedTournaments.length === 0 ? (
          <p className="rounded-lg border border-dashed border-border px-3 py-4 text-center text-xs text-muted">
            Keine archivierten Turniere
          </p>
        ) : (
          <ul className="space-y-2">
            {archivedTournaments.map((tournament) => (
              <li key={tournament.id}>
                <div
                  className={`group rounded-lg border transition-colors ${
                    selectedTournamentId === tournament.id
                      ? 'border-primary/60 bg-primary/10'
                      : 'border-border hover:bg-card-hover'
                  }`}
                >
                  <button
                    type="button"
                    onClick={() => onSelectTournament(tournament.id)}
                    className="block w-full p-3 text-left"
                  >
                    <div className="truncate text-sm font-medium text-foreground">
                      {tournament.name}
                    </div>
                    <div className="mt-1 flex items-center gap-2">
                      <Badge status={tournament.status} />
                      <span className="text-[10px] text-muted">
                        {new Date(tournament.created_at).toLocaleDateString('de-DE')}
                      </span>
                    </div>
                  </button>
                  <div className="border-t border-border/40 px-2 py-1.5 opacity-0 transition-opacity group-hover:opacity-100">
                    <button
                      type="button"
                      onClick={() => onDeleteArchived(tournament.id, tournament.name)}
                      disabled={deleteDisabled}
                      className="flex w-full items-center justify-center gap-1.5 rounded px-2 py-1 text-[11px] text-red-300 hover:bg-red-500/10 disabled:opacity-50"
                    >
                      <Trash2 size={11} />
                      Löschen
                    </button>
                  </div>
                </div>
              </li>
            ))}
          </ul>
        )}
      </Card>
    </aside>
  )
}
