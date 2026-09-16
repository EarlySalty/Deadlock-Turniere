import { Link } from 'react-router-dom'
import { motion } from 'framer-motion'
import { useTournaments } from '@/hooks/useTournament'
import Card from '@/components/ui/Card'
import Badge from '@/components/ui/Badge'
import LoadingSpinner from '@/components/ui/LoadingSpinner'
import { Trophy, Users, Archive, Swords, Shield, ScrollText } from 'lucide-react'

const container = {
  hidden: { opacity: 0 },
  show: {
    opacity: 1,
    transition: {
      staggerChildren: 0.1
    }
  }
}

const item = {
  hidden: { opacity: 0, y: 20 },
  show: { opacity: 1, y: 0 }
}

export default function Home() {
  const { data: tournaments, isLoading } = useTournaments()

  if (isLoading) return <LoadingSpinner />

  const active = tournaments?.find(t =>
    ['draft', 'registration', 'checkin', 'group_phase', 'bracket'].includes(t.status)
  )
  const archived = tournaments?.filter(t =>
    ['completed', 'archived'].includes(t.status)
  ) ?? []

  return (
    <motion.div
      variants={container}
      initial="hidden"
      animate="show"
      className="space-y-12 pb-20"
    >
      {/* Hero Section – gleiche visuelle Sprache wie die Streamer-Landing */}
      <section className="relative overflow-hidden rounded-2xl border border-border bg-card/55 px-6 py-16 text-center shadow-[var(--shadow-card-soft)] md:py-20">
        <div className="absolute inset-0 bg-gradient-to-b from-primary/8 via-transparent to-transparent" />
        <div className="relative z-10 mx-auto max-w-3xl">
          <motion.div
            variants={item}
            className="inline-flex items-center gap-2 rounded-full border border-border bg-background/70 px-4 py-1.5 text-sm font-medium text-accent"
          >
            <Swords size={15} />
            Turniere der Deutschen Deadlock Community
          </motion.div>
          <motion.h1 variants={item} className="mt-6 text-5xl font-bold tracking-tight text-foreground md:text-7xl font-display">
            Deadlock <span className="bg-gradient-to-r from-primary to-accent bg-clip-text text-transparent">Turniere</span>
          </motion.h1>
          <motion.p variants={item} className="mx-auto mt-6 max-w-2xl text-lg leading-relaxed text-muted md:text-xl">
            Anmeldung, Draft, Spielplan und Rangliste an einem Ort – für Community-Turniere ohne unnötigen Orga-Overhead.
          </motion.p>
        </div>
      </section>

      {/* Main Content */}
      <div className="grid grid-cols-1 lg:grid-cols-3 gap-8">
        {/* Active Tournament - Large Card */}
        <motion.section variants={item} className="lg:col-span-2 space-y-6">
          <h2 className="text-2xl font-bold flex items-center gap-3 tracking-widest text-primary/80">
            <Trophy size={24} />
            Aktives Geschehen
          </h2>

          {active ? (
            <Link to={`/${active.id}`}>
              <Card hoverable className="p-8 border-primary/20 bg-primary/5 group relative overflow-hidden">
                <div className="flex flex-col gap-6">
                  <div className="flex flex-col md:flex-row md:items-start justify-between gap-4">
                    <div className="space-y-2 flex-1">
                      <h3 className="text-3xl font-bold text-foreground group-hover:text-primary transition-colors leading-tight">
                        {active.name}
                      </h3>
                      {active.description && (
                        <p className="text-muted text-lg line-clamp-2 max-w-xl italic">
                          {active.description}
                        </p>
                      )}
                    </div>
                    <div className="shrink-0">
                      <Badge status={active.status} className="scale-110 origin-top-right" />
                    </div>
                  </div>

                  <div className="flex flex-wrap items-center gap-6 text-sm text-muted uppercase tracking-widest font-bold">
                    <span className="flex items-center gap-2">
                      <Users size={16} className="text-primary" />
                      {active.team_size}er Teams
                    </span>
                    <span className="flex items-center gap-2">
                      <Shield size={16} className="text-primary" />
                      {active.bracket_format === 'single_elimination' ? 'Single Elim.' : 'Double Elim.'}
                    </span>
                    {active.rules && (
                      <span className="flex items-center gap-2 text-amber-500/80">
                        <ScrollText size={16} />
                        Regelwerk bereit
                      </span>
                    )}
                  </div>

                  <div className="pt-4">
                    <span className="inline-flex items-center gap-2 text-primary font-bold uppercase tracking-widest text-xs group-hover:gap-4 transition-all">
                      Jetzt teilnehmen <Swords size={14} />
                    </span>
                  </div>
                </div>
              </Card>
            </Link>
          ) : (
            <Card className="text-center py-20 opacity-60 italic">
              <Trophy size={48} className="mx-auto text-muted mb-4 opacity-20" />
              <p className="text-muted text-xl">Die Arena ist momentan still.</p>
              <p className="text-muted text-sm mt-2 uppercase tracking-widest">Warten auf die nächste Beschwörung</p>
            </Card>
          )}
        </motion.section>

        {/* Sidebar / Archive */}
        <motion.section variants={item} className="space-y-6">
          <h2 className="text-2xl font-bold flex items-center gap-3 tracking-widest text-muted">
            <Archive size={20} />
            Chroniken
          </h2>

          <div className="flex flex-col gap-4">
            {archived.length > 0 ? archived.map(t => (
              <Link key={t.id} to={`/${t.id}`} className="relative block group">
                <Card hoverable className="p-4 border-white/5 bg-white/[0.02] hover:z-10 relative">
                  <div className="flex items-center justify-between gap-4">
                    <div className="min-w-0">
                      <h3 className="font-bold text-foreground truncate uppercase text-sm tracking-wide group-hover:text-primary transition-colors">
                        {t.name}
                      </h3>
                      <span className="text-[10px] text-muted uppercase tracking-tighter">
                        {new Date(t.created_at).toLocaleDateString('de-DE')}
                      </span>
                    </div>
                    <Badge status={t.status} className="scale-75 origin-right" />
                  </div>
                </Card>
              </Link>
            )) : (
              <p className="text-center py-10 text-muted italic text-sm">Noch keine Geschichte geschrieben.</p>
            )}
          </div>
        </motion.section>
      </div>
    </motion.div>
  )
}
