import { Link } from 'react-router-dom'
import { Trophy, Medal, Star, Swords, Target } from 'lucide-react'
import { useLeaderboard } from '@/hooks/useTournament'
import Card from '@/components/ui/Card'
import LoadingSpinner from '@/components/ui/LoadingSpinner'

function placementLabel(placement: number | null): string {
  if (!placement) return '—'
  if (placement === 1) return '1. Platz'
  if (placement === 2) return '2. Platz'
  if (placement <= 4) return 'Halbfinale'
  return `Platz ${placement}`
}

function RankMedal({ position }: { position: number }) {
  if (position === 1) return <Trophy size={18} className="text-yellow-400" />
  if (position === 2) return <Medal size={18} className="text-slate-300" />
  if (position === 3) return <Medal size={18} className="text-amber-600" />
  return <span className="text-sm font-bold text-muted w-4.5 text-center">{position}</span>
}

export default function Leaderboard() {
  const { data: entries, isLoading, isError } = useLeaderboard()

  if (isLoading) return <LoadingSpinner />

  if (isError || !entries) {
    return (
      <Card className="text-center py-10">
        <p className="text-muted">Rangliste konnte nicht geladen werden.</p>
      </Card>
    )
  }

  return (
    <div className="space-y-6">
      <div>
        <div className="flex items-center gap-3 mb-1">
          <Trophy size={24} className="text-primary" />
          <h1 className="text-2xl font-bold text-foreground">Globale Rangliste</h1>
        </div>
        <p className="text-sm text-muted">
          Alle Spieler sortiert nach gesammelten Turnierpunkten.
        </p>
      </div>

      {entries.length === 0 ? (
        <Card className="text-center py-12">
          <Trophy size={40} className="mx-auto text-muted mb-3" />
          <p className="text-muted">Noch keine Turnierdaten vorhanden.</p>
          <p className="text-xs text-muted mt-1">Punkte werden nach Turnier-Abschluss berechnet.</p>
        </Card>
      ) : (
        <Card className="overflow-hidden">
          <div className="overflow-x-auto">
            <table className="w-full text-sm">
              <thead>
                <tr className="border-b border-border bg-background/50">
                  <th className="px-4 py-3 text-left font-medium text-muted w-12">#</th>
                  <th className="px-4 py-3 text-left font-medium text-muted">Spieler</th>
                  <th className="px-4 py-3 text-left font-medium text-muted hidden sm:table-cell">Rang</th>
                  <th className="px-4 py-3 text-right font-medium text-muted">
                    <span className="flex items-center justify-end gap-1">
                      <Star size={13} />
                      Punkte
                    </span>
                  </th>
                  <th className="px-4 py-3 text-right font-medium text-muted hidden md:table-cell">
                    <span className="flex items-center justify-end gap-1">
                      <Swords size={13} />
                      Matches
                    </span>
                  </th>
                  <th className="px-4 py-3 text-right font-medium text-muted hidden md:table-cell">
                    <span className="flex items-center justify-end gap-1">
                      <Target size={13} />
                      Siege
                    </span>
                  </th>
                  <th className="px-4 py-3 text-right font-medium text-muted hidden lg:table-cell">Bestes Ergebnis</th>
                </tr>
              </thead>
              <tbody>
                {entries.map((entry) => (
                  <tr
                    key={entry.rank_position}
                    className={`border-b border-border/50 transition-colors hover:bg-background/40 ${
                      entry.rank_position <= 3 ? 'bg-primary/5' : ''
                    }`}
                  >
                    <td className="px-4 py-3">
                      <div className="flex items-center justify-center">
                        <RankMedal position={entry.rank_position} />
                      </div>
                    </td>
                    <td className="px-4 py-3">
                      <Link
                        to={`/spieler/${encodeURIComponent(entry.discord_name)}`}
                        className="font-medium text-foreground hover:text-primary transition-colors"
                      >
                        {entry.discord_name}
                      </Link>
                    </td>
                    <td className="px-4 py-3 hidden sm:table-cell">
                      {entry.rank ? (
                        <span className="text-xs px-2 py-0.5 rounded-full bg-primary/15 text-primary font-medium">
                          {entry.rank}
                        </span>
                      ) : (
                        <span className="text-muted text-xs">—</span>
                      )}
                    </td>
                    <td className="px-4 py-3 text-right font-bold text-foreground">
                      {entry.total_points}
                    </td>
                    <td className="px-4 py-3 text-right text-muted hidden md:table-cell">
                      {entry.matches_played}
                    </td>
                    <td className="px-4 py-3 text-right text-muted hidden md:table-cell">
                      {entry.matches_won}
                    </td>
                    <td className="px-4 py-3 text-right text-muted hidden lg:table-cell text-xs">
                      {placementLabel(entry.best_placement)}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </Card>
      )}

      <Card className="p-4 text-xs text-muted space-y-1">
        <p className="font-medium text-foreground text-sm mb-2">Punkte-System</p>
        <div className="grid grid-cols-2 sm:grid-cols-3 gap-2">
          <span>Turniersieg (Platz 1): <strong className="text-foreground">10 Pkt.</strong></span>
          <span>Finalist (Platz 2): <strong className="text-foreground">6 Pkt.</strong></span>
          <span>Halbfinale (Platz 3–4): <strong className="text-foreground">3 Pkt.</strong></span>
          <span>Teilnahme: <strong className="text-foreground">1 Pkt.</strong></span>
          <span>Pro Match-Sieg: <strong className="text-foreground">+0,5 Pkt.</strong></span>
        </div>
      </Card>
    </div>
  )
}
