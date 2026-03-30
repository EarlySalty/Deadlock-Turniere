import { Link } from 'react-router-dom'
import { useTournaments } from '@/hooks/useTournament'
import Card from '@/components/ui/Card'
import Badge from '@/components/ui/Badge'
import LoadingSpinner from '@/components/ui/LoadingSpinner'
import { Trophy, Calendar, Users, Archive } from 'lucide-react'

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
    <div className="space-y-8">
      {/* Hero */}
      <div className="text-center py-12">
        <Trophy size={48} className="mx-auto text-primary mb-4" />
        <h1 className="text-3xl font-bold text-foreground mb-2">Deadlock Turniere</h1>
        <p className="text-muted text-lg">Deutsche Deadlock Community — Turnierplattform</p>
      </div>

      {/* Aktives Turnier */}
      {active ? (
        <section>
          <h2 className="text-xl font-semibold text-foreground mb-4 flex items-center gap-2">
            <Calendar size={20} className="text-primary" />
            Aktives Turnier
          </h2>
          <Link to={`/turnier/${active.id}`}>
            <Card hoverable className="p-6">
              <div className="flex items-center justify-between">
                <div>
                  <h3 className="text-lg font-bold text-foreground">{active.name}</h3>
                  {active.description && (
                    <p className="text-muted mt-1">{active.description}</p>
                  )}
                  <div className="flex items-center gap-4 mt-3 text-sm text-muted">
                    <span className="flex items-center gap-1">
                      <Users size={14} />
                      {active.team_size}er Teams
                    </span>
                    <span>{active.bracket_format === 'single_elimination' ? 'Single Elimination' : 'Double Elimination'}</span>
                  </div>
                </div>
                <Badge status={active.status} />
              </div>
            </Card>
          </Link>
        </section>
      ) : (
        <Card className="text-center py-10">
          <Trophy size={32} className="mx-auto text-muted mb-3" />
          <p className="text-muted text-lg">Aktuell kein Turnier aktiv</p>
          <p className="text-muted text-sm mt-1">Schau bald wieder vorbei!</p>
        </Card>
      )}

      {/* Archiv */}
      {archived.length > 0 && (
        <section>
          <h2 className="text-xl font-semibold text-foreground mb-4 flex items-center gap-2">
            <Archive size={20} className="text-muted" />
            Vergangene Turniere
          </h2>
          <div className="grid gap-3">
            {archived.map(t => (
              <Link key={t.id} to={`/turnier/${t.id}`}>
                <Card hoverable className="p-4">
                  <div className="flex items-center justify-between">
                    <div>
                      <h3 className="font-medium text-foreground">{t.name}</h3>
                      <span className="text-sm text-muted">
                        {new Date(t.created_at).toLocaleDateString('de-DE')}
                      </span>
                    </div>
                    <Badge status={t.status} />
                  </div>
                </Card>
              </Link>
            ))}
          </div>
        </section>
      )}
    </div>
  )
}
