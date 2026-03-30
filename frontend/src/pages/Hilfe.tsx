import Card from '@/components/ui/Card'
import { BookOpen, Bot, HelpCircle, Settings, Trophy } from 'lucide-react'

const TOURNAMENT_STEPS = [
  {
    title: '1. Turnier erstellen',
    text: 'Admins legen Name, Beschreibung, Teamgröße und die relevanten Zeitpunkte für Anmeldung, Gruppenphase und Bracket fest.',
  },
  {
    title: '2. Anmeldephase',
    text: 'Spieler melden sich solo an oder erstellen direkt ein Team. Solo-Spieler können später auch von Captains eingeladen werden.',
  },
  {
    title: '3. Gruppenphase',
    text: 'Die Teams werden auf Gruppen verteilt und spielen Round-Robin. Die besten zwei Teams pro Gruppe qualifizieren sich für das Bracket.',
  },
  {
    title: '4. Bracket',
    text: 'In der K.O.-Phase geht es Runde für Runde bis ins Finale. Gewinner werden automatisch in die nächste Runde weitergetragen.',
  },
  {
    title: '5. Abschluss',
    text: 'Nach dem Finale bleibt das Turnier als Ergebnisübersicht sichtbar und kann später archiviert werden.',
  },
]

const ADMIN_ACTIONS = [
  ['Anmeldung starten', 'Wenn das Turnier freigegeben werden soll oder der Auto-Scheduler übersteuert werden muss.', 'Setzt den Status auf Anmeldung, damit Teams und Solo-Spieler beitreten können.'],
  ['Zur Gruppenphase', 'Wenn die Anmeldung beendet ist und die Gruppen sofort erzeugt werden sollen.', 'Wechselt in die Gruppenphase und generiert automatisch Gruppen plus Gruppenspiele.'],
  ['Zum Bracket', 'Wenn die Gruppenphase abgeschlossen ist oder manuell ins K.O.-Bracket gewechselt werden soll.', 'Erstellt das Bracket aus den Gruppen-Ergebnissen und setzt das Turnier in die Bracket-Phase.'],
  ['Lobby erstellen', 'Sobald ein Bracket-Match bereit ist.', 'Fordert den Steam-Bot an, eine Lobby anzulegen, und speichert den Party-Code im Admin-Bereich.'],
  ['Ergebnis abrufen', 'Wenn ein Match im Spiel gelaufen ist und Deadlock-Daten verfügbar sind.', 'Lädt das Ergebnis automatisch vom Deadlock-/Steam-Flow und übernimmt den Sieger.'],
  ['Ergebnis manuell', 'Wenn Auto-Fetch fehlschlägt oder ein Gruppenspiel direkt gepflegt werden soll.', 'Setzt den Sieger manuell und aktualisiert Tabelle beziehungsweise Bracket.'],
]

const BOT_HELP = [
  'Spieler melden sich während der Anmeldephase solo an oder treten einem Team bei.',
  'Captains können Teams erstellen, Mitglieder verwalten und offene Solo-Spieler direkt einladen.',
  'Ergebnisse und Sieger sind nach der Eintragung auf der Turnierseite in Gruppen, Bracket und Ergebnisübersicht sichtbar.',
]

export default function Hilfe() {
  return (
    <div className="space-y-6">
      <div>
        <h1 className="text-2xl font-bold text-foreground">Hilfe & Dokumentation</h1>
        <p className="mt-2 max-w-3xl text-sm text-muted">
          Übersicht für Admins und Teilnehmer, die den Ablauf, die wichtigsten Buttons und die
          sichtbaren Ergebnisse der Turnierseite schnell verstehen wollen.
        </p>
      </div>

      <details open className="group">
        <summary className="list-none cursor-pointer">
          <Card className="flex items-center gap-3 p-4">
            <Trophy size={18} className="text-primary" />
            <div>
              <h2 className="text-lg font-semibold text-foreground">Turnier-Ablauf</h2>
              <p className="text-sm text-muted">Vom Draft bis zum Abschluss in fünf klaren Phasen.</p>
            </div>
          </Card>
        </summary>
        <div className="mt-4 grid gap-3 md:grid-cols-5">
          {TOURNAMENT_STEPS.map((step) => (
            <Card key={step.title} className="p-4">
              <div className="mb-3 h-1 rounded-full bg-primary/70" />
              <h3 className="text-sm font-semibold text-foreground">{step.title}</h3>
              <p className="mt-2 text-sm text-muted">{step.text}</p>
            </Card>
          ))}
        </div>
      </details>

      <details className="group">
        <summary className="list-none cursor-pointer">
          <Card className="flex items-center gap-3 p-4">
            <Settings size={18} className="text-primary" />
            <div>
              <h2 className="text-lg font-semibold text-foreground">Admin-Buttons erklärt</h2>
              <p className="text-sm text-muted">Wann welcher Button sinnvoll ist und was er im System auslöst.</p>
            </div>
          </Card>
        </summary>
        <Card className="mt-4 overflow-hidden p-0">
          <div className="overflow-x-auto">
            <table className="w-full text-sm">
              <thead>
                <tr className="border-b border-border text-left text-muted">
                  <th className="px-4 py-3">Button</th>
                  <th className="px-4 py-3">Wann drücken</th>
                  <th className="px-4 py-3">Was passiert</th>
                </tr>
              </thead>
              <tbody>
                {ADMIN_ACTIONS.map(([label, when, effect]) => (
                  <tr key={label} className="border-b border-border/50 last:border-0">
                    <td className="px-4 py-3 font-medium text-foreground">{label}</td>
                    <td className="px-4 py-3 text-muted">{when}</td>
                    <td className="px-4 py-3 text-muted">{effect}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </Card>
      </details>

      <details className="group">
        <summary className="list-none cursor-pointer">
          <Card className="flex items-center gap-3 p-4">
            <Bot size={18} className="text-primary" />
            <div>
              <h2 className="text-lg font-semibold text-foreground">Bot-Befehle & Nutzung</h2>
              <p className="text-sm text-muted">Kurze Orientierung für Anmeldung, Team-Handling und Ergebnisanzeige.</p>
            </div>
          </Card>
        </summary>
        <div className="mt-4 grid gap-3 md:grid-cols-3">
          <Card className="p-4">
            <div className="flex items-center gap-2 text-sm font-semibold text-foreground">
              <BookOpen size={16} className="text-primary" />
              Anmeldung
            </div>
            <p className="mt-2 text-sm text-muted">{BOT_HELP[0]}</p>
          </Card>
          <Card className="p-4">
            <div className="flex items-center gap-2 text-sm font-semibold text-foreground">
              <HelpCircle size={16} className="text-primary" />
              Teams
            </div>
            <p className="mt-2 text-sm text-muted">{BOT_HELP[1]}</p>
          </Card>
          <Card className="p-4">
            <div className="flex items-center gap-2 text-sm font-semibold text-foreground">
              <Trophy size={16} className="text-primary" />
              Ergebnisse
            </div>
            <p className="mt-2 text-sm text-muted">{BOT_HELP[2]}</p>
          </Card>
        </div>
      </details>
    </div>
  )
}
