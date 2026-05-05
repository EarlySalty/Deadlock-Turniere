import { Link } from 'react-router-dom'
import { motion } from 'framer-motion'
import { Trophy, Medal, Star, ArrowUpRight } from 'lucide-react'
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
  if (position === 1) return <Trophy size={20} className="text-primary shadow-[var(--glow-primary)]" />
  if (position === 2) return <Medal size={20} className="text-slate-300" />
  if (position === 3) return <Medal size={20} className="text-amber-600" />
  return <span className={`text-sm font-bold w-6 text-center ${position <= 10 ? 'text-foreground' : 'text-muted'}`}>
    {position.toString().padStart(2, '0')}
  </span>
}

export default function Leaderboard() {
  const { data: entries, isLoading, isError } = useLeaderboard()

  if (isLoading) return <LoadingSpinner />

  if (isError || !entries) {
    return (
      <Card className="text-center py-10 opacity-60">
        <p className="text-muted italic">Die Chroniken konnten nicht beschworen werden.</p>
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
          <h1 className="text-4xl font-bold tracking-tighter text-foreground font-display">Hall of <span className="text-primary">Legends</span></h1>
          <p className="text-muted italic">"Diejenigen, die in der Arena unsterblich wurden."</p>
        </div>
        <div className="hidden md:flex gap-4">
          <div className="px-4 py-2 rounded-lg border border-white/5 bg-white/5">
            <p className="text-[10px] uppercase font-bold text-muted tracking-widest">Status</p>
            <p className="text-sm font-bold text-foreground">BETA-PHASE</p>
          </div>
        </div>
      </div>

      {entries.length === 0 ? (
        <Card className="text-center py-20 opacity-40 italic">
          <Trophy size={48} className="mx-auto text-muted mb-4 opacity-20" />
          <p className="text-muted text-xl">Noch wurde keine Geschichte geschrieben.</p>
          <p className="text-muted text-sm mt-2 uppercase tracking-widest">Die Arena wartet auf ihren ersten Helden</p>
        </Card>
      ) : (
        <Card className="p-0 overflow-hidden border-white/5">
          <div className="overflow-x-auto">
            <table className="w-full text-sm">
              <thead>
                <tr className="bg-white/5 text-left border-b border-white/10">
                  <th className="px-6 py-4 font-bold uppercase tracking-widest text-muted text-[10px] w-16 text-center">Rang</th>
                  <th className="px-6 py-4 font-bold uppercase tracking-widest text-muted text-[10px]">Held</th>
                  <th className="px-6 py-4 font-bold uppercase tracking-widest text-muted text-[10px] hidden sm:table-cell">Rang-Stufe</th>
                  <th className="px-6 py-4 font-bold uppercase tracking-widest text-muted text-[10px] text-right">
                    <span className="inline-flex items-center gap-1.5">
                      <Star size={12} className="text-primary" />
                      Punkte
                    </span>
                  </th>
                  <th className="px-6 py-4 font-bold uppercase tracking-widest text-muted text-[10px] text-right hidden md:table-cell">
                    Einsätze
                  </th>
                  <th className="px-6 py-4 font-bold uppercase tracking-widest text-muted text-[10px] text-right hidden md:table-cell">
                    Siege
                  </th>
                  <th className="px-6 py-4 font-bold uppercase tracking-widest text-muted text-[10px] text-right hidden lg:table-cell">Bestes Resultat</th>
                  <th className="px-6 py-4 w-16"></th>
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
                    <td className="px-6 py-4">
                      <div className="flex items-center justify-center">
                        <RankMedal position={entry.rank_position} />
                      </div>
                    </td>
                    <td className="px-6 py-4">
                      <Link
                        to={`/spieler/${encodeURIComponent(entry.discord_name)}`}
                        className="font-bold text-foreground hover:text-primary transition-colors uppercase tracking-wide"
                      >
                        {entry.discord_name}
                      </Link>
                    </td>
                    <td className="px-6 py-4 hidden sm:table-cell">
                      {entry.rank ? (
                        <span className="text-[10px] font-bold text-muted uppercase tracking-widest bg-white/5 px-2 py-0.5 rounded-lg border border-white/5">
                          {entry.rank}
                        </span>
                      ) : (
                        <span className="text-muted text-[10px] uppercase tracking-widest">—</span>
                      )}
                    </td>
                    <td className="px-6 py-4 text-right font-bold text-primary text-base">
                      {entry.total_points}
                    </td>
                    <td className="px-6 py-4 text-right text-muted hidden md:table-cell font-medium">
                      {entry.matches_played}
                    </td>
                    <td className="px-6 py-4 text-right text-muted hidden md:table-cell font-medium">
                      {entry.matches_won}
                    </td>
                    <td className="px-6 py-4 text-right text-muted hidden lg:table-cell text-[10px] uppercase font-bold tracking-wider">
                      {placementLabel(entry.best_placement)}
                    </td>
                    <td className="px-6 py-4 text-right">
                      <Link to={`/spieler/${encodeURIComponent(entry.discord_name)}`}>
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
            <p className="text-[10px] uppercase font-bold text-primary tracking-widest">Sieg</p>
            <p className="text-sm text-foreground font-medium">10 Punkte für Platz 1</p>
          </div>
          <div className="space-y-1">
            <p className="text-[10px] uppercase font-bold text-primary tracking-widest">Finalist</p>
            <p className="text-sm text-foreground font-medium">6 Punkte für Platz 2</p>
          </div>
          <div className="space-y-1">
            <p className="text-[10px] uppercase font-bold text-primary tracking-widest">Halbfinale</p>
            <p className="text-sm text-foreground font-medium">3 Punkte für Platz 3–4</p>
          </div>
          <div className="space-y-1">
            <p className="text-[10px] uppercase font-bold text-primary tracking-widest">Boni</p>
            <p className="text-sm text-foreground font-medium">+0,5 Pkt. pro Match-Sieg</p>
          </div>
        </div>
      </Card>
    </motion.div>
  )
}
