import { Link } from 'react-router-dom'
import { motion } from 'framer-motion'
import { Trophy, Medal, Star, ArrowUpRight } from 'lucide-react'
import { useLeaderboard } from '@/hooks/useTournament'
import Card from '@/components/ui/Card'
import LoadingSpinner from '@/components/ui/LoadingSpinner'
import { placementLabel } from '@/lib/bracketPlacements'

function RankMedal({ position }: { position: number }) {
  if (position === 1) return <Trophy size={20} className="text-primary shadow-[var(--glow-primary)]" />
  if (position === 2) return <Medal size={20} className="text-slate-300" />
  if (position === 3) return <Medal size={20} className="text-amber-600" />
  return <span className="w-6 text-center text-sm font-bold tabular-nums text-foreground">{position}</span>
}

export default function Leaderboard() {
  const { data: entries, isLoading, isError } = useLeaderboard()

  if (isLoading) return <LoadingSpinner />

  if (isError || !entries) {
    return (
      <Card className="text-center py-10 opacity-60">
        <p className="text-muted italic">Die Rangliste konnte nicht geladen werden.</p>
      </Card>
    )
  }

  return (
    <motion.div
      initial={{ opacity: 0, y: 10 }}
      animate={{ opacity: 1, y: 0 }}
      className="space-y-8 pb-20"
    >
      <div className="flex flex-col md:flex-row md:items-end justify-between gap-6 border-b border-white/5 pb-8">
        <div className="space-y-2">
          <h1 className="text-4xl font-bold tracking-tighter text-foreground font-display">Die <span className="text-primary">Rangliste</span></h1>
          <p className="text-muted">Die besten Spieler aus unseren Turnieren. Die Rangliste ist noch in der Testphase.</p>
        </div>
      </div>

      {entries.length === 0 ? (
        <Card className="text-center py-20 opacity-40 italic">
          <Trophy size={48} className="mx-auto text-muted mb-4 opacity-20" />
          <p className="text-muted text-xl">Noch keine Ergebnisse.</p>
          <p className="text-muted text-sm mt-2 uppercase tracking-widest">Die Rangliste füllt sich nach dem ersten Turnier</p>
        </Card>
      ) : (
        <Card className="p-0 overflow-hidden border-white/5">
          <div className="overflow-x-auto">
            <table className="w-full text-sm">
              <thead>
                <tr className="bg-white/5 text-left border-b border-white/10">
                  <th className="px-3 py-3 sm:px-6 sm:py-4 font-bold uppercase tracking-widest text-muted text-xs w-16 text-center">Rang</th>
                  <th className="px-3 py-3 sm:px-6 sm:py-4 font-bold uppercase tracking-widest text-muted text-xs">Spieler</th>
                  <th className="px-3 py-3 sm:px-6 sm:py-4 font-bold uppercase tracking-widest text-muted text-xs hidden sm:table-cell">Deadlock-Rang</th>
                  <th className="px-3 py-3 sm:px-6 sm:py-4 font-bold uppercase tracking-widest text-muted text-xs text-right">
                    <span className="inline-flex items-center gap-1.5">
                      <Star size={12} className="text-primary" />
                      Punkte
                    </span>
                  </th>
                  <th className="px-3 py-3 sm:px-6 sm:py-4 font-bold uppercase tracking-widest text-muted text-xs text-right hidden md:table-cell">
                    Matches
                  </th>
                  <th className="px-3 py-3 sm:px-6 sm:py-4 font-bold uppercase tracking-widest text-muted text-xs text-right hidden md:table-cell">
                    Siege
                  </th>
                  <th className="px-3 py-3 sm:px-6 sm:py-4 font-bold uppercase tracking-widest text-muted text-xs text-right hidden lg:table-cell">Bestes Resultat</th>
                  <th className="hidden w-16 px-3 py-3 sm:px-6 sm:py-4 sm:table-cell"><span className="sr-only">Profil</span></th>
                </tr>
              </thead>
              <tbody>
                {entries.map((entry) => (
                  <tr
                    key={entry.rank_position}
                    className={`border-b border-white/5 last:border-0 transition-colors hover:bg-white/[0.02] ${
                      entry.rank_position <= 3 ? 'bg-primary/[0.03]' : ''
                    }`}
                  >
                    <td className="px-3 py-3 sm:px-6 sm:py-4">
                      <div className="flex items-center justify-center">
                        <RankMedal position={entry.rank_position} />
                      </div>
                    </td>
                    <td className="px-3 py-3 sm:px-6 sm:py-4">
                      <Link
                        to={`/spieler/${encodeURIComponent(entry.discord_name)}`}
                        className="break-words font-bold text-foreground transition-colors hover:text-primary"
                      >
                        {entry.discord_name}
                      </Link>
                    </td>
                    <td className="px-3 py-3 sm:px-6 sm:py-4 hidden sm:table-cell">
                      {entry.rank ? (
                        <span className="text-xs font-bold text-muted uppercase tracking-widest bg-white/5 px-2 py-0.5 rounded-lg border border-white/5">
                          {entry.rank}
                        </span>
                      ) : (
                        <span className="text-muted text-xs uppercase tracking-widest">-</span>
                      )}
                    </td>
                    <td className="px-3 py-3 sm:px-6 sm:py-4 text-right font-bold tabular-nums text-primary text-base">
                      {entry.total_points}
                    </td>
                    <td className="px-3 py-3 sm:px-6 sm:py-4 text-right text-muted hidden md:table-cell font-medium">
                      {entry.matches_played}
                    </td>
                    <td className="px-3 py-3 sm:px-6 sm:py-4 text-right text-muted hidden md:table-cell font-medium">
                      {entry.matches_won}
                    </td>
                    <td className="px-3 py-3 sm:px-6 sm:py-4 text-right text-muted hidden lg:table-cell text-xs uppercase font-bold tracking-wider">
                      {placementLabel(entry.best_placement, entry.best_placement_shared, '-')}
                    </td>
                    <td className="hidden px-3 py-3 sm:px-6 sm:py-4 text-right sm:table-cell">
                      <Link to={`/spieler/${encodeURIComponent(entry.discord_name)}`} aria-label={`Profil von ${entry.discord_name}`}>
                        <ArrowUpRight size={16} className="text-muted hover:text-primary transition-colors cursor-pointer ml-auto" />
                      </Link>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </Card>
      )}

      <Card className="p-6 border-white/5 bg-white/[0.02]">
        <h3 className="font-display font-bold text-sm tracking-[0.2em] text-foreground mb-4 border-b border-white/5 pb-2">Das Punktesystem</h3>
        <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-6">
          <div className="space-y-1">
            <p className="text-xs uppercase font-bold text-primary tracking-widest">Sieg</p>
            <p className="text-sm text-foreground font-medium">10 Punkte für Platz 1</p>
          </div>
          <div className="space-y-1">
            <p className="text-xs uppercase font-bold text-primary tracking-widest">Finalist</p>
            <p className="text-sm text-foreground font-medium">6 Punkte für Platz 2</p>
          </div>
          <div className="space-y-1">
            <p className="text-xs uppercase font-bold text-primary tracking-widest">3. und 4. Platz</p>
            <p className="text-sm text-foreground font-medium">3 Punkte für Platz 3 und 4</p>
          </div>
          <div className="space-y-1">
            <p className="text-xs uppercase font-bold text-primary tracking-widest">Boni</p>
            <p className="text-sm text-foreground font-medium">+0,5 Pkt. pro Match-Sieg</p>
          </div>
        </div>
      </Card>
    </motion.div>
  )
}
