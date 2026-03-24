import { useTournaments } from '@/hooks/useTournament'
import Card from '@/components/ui/Card'
import Badge from '@/components/ui/Badge'
import Button from '@/components/ui/Button'
import LoadingSpinner from '@/components/ui/LoadingSpinner'
import { Plus, Settings, Archive } from 'lucide-react'

export default function Admin() {
  const { data: tournaments, isLoading } = useTournaments()

  if (isLoading) return <LoadingSpinner />

  const active = tournaments?.find(t =>
    ['draft', 'registration', 'group_phase', 'bracket'].includes(t.status)
  )
  const archived = tournaments?.filter(t =>
    ['completed', 'archived'].includes(t.status)
  ) ?? []

  return (
    <div className="space-y-8">
      <div className="flex items-center justify-between">
        <h1 className="text-2xl font-bold text-foreground">Turnier-Verwaltung</h1>
        <Button variant="primary">
          <Plus size={16} />
          Neues Turnier
        </Button>
      </div>

      {/* Aktives Turnier verwalten */}
      <section>
        <h2 className="text-lg font-semibold text-foreground mb-3 flex items-center gap-2">
          <Settings size={18} className="text-primary" />
          Aktives Turnier
        </h2>
        {active ? (
          <Card className="p-5">
            <div className="flex items-center justify-between mb-4">
              <div>
                <h3 className="font-bold text-foreground">{active.name}</h3>
                <Badge status={active.status} />
              </div>
            </div>
            <div className="grid grid-cols-2 sm:grid-cols-4 gap-3">
              <Card className="p-3 text-center">
                <div className="text-2xl font-bold text-primary">{active.team_size}</div>
                <div className="text-xs text-muted">Teamgroesse</div>
              </Card>
              <Card className="p-3 text-center">
                <div className="text-2xl font-bold text-primary">—</div>
                <div className="text-xs text-muted">Teams</div>
              </Card>
              <Card className="p-3 text-center">
                <div className="text-2xl font-bold text-primary">—</div>
                <div className="text-xs text-muted">Spieler</div>
              </Card>
              <Card className="p-3 text-center">
                <div className="text-2xl font-bold text-primary">—</div>
                <div className="text-xs text-muted">Matches</div>
              </Card>
            </div>
            <div className="mt-4 flex gap-2">
              <Button variant="secondary" size="sm">Einstellungen</Button>
              <Button variant="secondary" size="sm">Teams verwalten</Button>
              <Button variant="secondary" size="sm">Phase weiterschalten</Button>
            </div>
          </Card>
        ) : (
          <Card className="p-6 text-center">
            <p className="text-muted">Kein aktives Turnier. Erstelle ein neues Turnier um zu starten.</p>
          </Card>
        )}
      </section>

      {/* Archiv */}
      {archived.length > 0 && (
        <section>
          <h2 className="text-lg font-semibold text-foreground mb-3 flex items-center gap-2">
            <Archive size={18} className="text-muted" />
            Archiv ({archived.length})
          </h2>
          <div className="grid gap-2">
            {archived.map(t => (
              <Card key={t.id} className="p-3 flex items-center justify-between">
                <div>
                  <span className="font-medium text-foreground">{t.name}</span>
                  <span className="ml-3 text-sm text-muted">
                    {new Date(t.created_at).toLocaleDateString('de-DE')}
                  </span>
                </div>
                <Badge status={t.status} />
              </Card>
            ))}
          </div>
        </section>
      )}
    </div>
  )
}
