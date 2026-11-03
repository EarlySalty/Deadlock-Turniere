import { motion } from 'framer-motion'
import Card from '@/components/ui/Card'
import { DISCORD_INVITE_URL, TOURNAMENT_RULES_URL } from '@/lib/discord'
import {
  BookOpen, Settings, Trophy, ScrollText,
  AlertTriangle
} from 'lucide-react'

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

const LEAGUE_STEPS = [
  'Beim Erstellen wählst du das Format „Jeder gegen jeden“ und den Start der ersten Runde.',
  'Beim Abschluss des Check-ins entsteht der Spielplan: jedes Team spielt einmal gegen jedes andere, in Runden zu festen Zeiten. Bei 10 Teams sind das 45 Spiele in 9 Runden. Bei ungerader Teamzahl setzt pro Runde ein Team aus.',
  'Unter „Spielplan“ änderst du Start und Minuten pro Runde, trägst Sieger ein und korrigierst sie. Lobbys öffnen zum Termin der aktuellen Runde.',
  'Ein Sieg gibt +2 Punkte, eine Niederlage -1. Bei Punktgleichheit entscheidet die Zahl der Siege, danach der direkte Vergleich.',
  'Sind alle Spiele gewertet, schließt du das Turnier ab. Die Tabelle ist die Endplatzierung, ein Bracket gibt es nicht.',
]

const ADMIN_ACTIONS = [
  ['Anmeldung starten', 'Wenn das Turnier freigegeben werden soll.', 'Setzt den Status auf Anmeldung.'],
  ['Zur Gruppenphase', 'Wenn die Anmeldung beendet ist.', 'Generiert Gruppen und Spiele.'],
  ['Zum Bracket', 'Wenn die Gruppenphase abgeschlossen ist.', 'Erstellt das K.O.-Bracket.'],
  ['Lobby erstellen', 'Sobald ein Bracket-Match bereit ist.', 'Fordert den Steam-Bot an.'],
  ['Ergebnis abrufen', 'Wenn ein Match im Spiel gelaufen ist.', 'Lädt Daten vom Steam-Flow.'],
  ['Ergebnis manuell', 'Wenn Auto-Fetch fehlschlägt.', 'Setzt den Sieger manuell.'],
]

export default function Hilfe() {
  return (
    <motion.div
      initial={{ opacity: 0, y: 10 }}
      animate={{ opacity: 1, y: 0 }}
      className="max-w-4xl mx-auto space-y-12 pb-20"
    >
      <div className="text-center space-y-4 border-b border-white/5 pb-12">
        <div className="flex justify-center">
          <div className="p-3 rounded-lg bg-primary/10 border border-primary/20">
            <BookOpen size={32} className="text-primary" />
          </div>
        </div>
        <h1 className="text-4xl font-bold tracking-tighter text-foreground font-display">Turnier-<span className="text-primary">Hilfe</span></h1>
        <p className="text-muted italic max-w-lg mx-auto">
          So laufen unsere Turniere ab.
        </p>
      </div>

      <Card className="p-8 text-center space-y-4">
        <ScrollText size={32} className="mx-auto text-primary" />
        <h2 className="text-xl font-bold text-foreground uppercase tracking-widest">Regelwerk</h2>
        <p className="text-sm text-muted max-w-md mx-auto">
          Die aktuellen Turnierregeln findest du auf unserem Discord im Kanal #turnier-regeln.
        </p>
        <div className="pt-2">
          <a
            href={TOURNAMENT_RULES_URL}
            target="_blank"
            rel="noopener noreferrer"
            className="inline-flex items-center gap-2 bg-primary hover:bg-primary-hover text-on-gold px-8 py-3 rounded-lg font-bold uppercase tracking-widest text-xs transition-colors"
          >
            Regeln auf Discord lesen
          </a>
        </div>
      </Card>

      {/* Turnier-Ablauf */}
      <section className="space-y-6">
        <div className="flex items-center gap-3 border-l-2 border-primary pl-4 py-1">
          <Trophy size={20} className="text-primary" />
          <h2 className="text-xl font-bold tracking-widest text-foreground uppercase">Turnier-Phasen</h2>
        </div>
        <div className="grid grid-cols-1 md:grid-cols-5 gap-4">
          {TOURNAMENT_STEPS.map((step, idx) => (
            <Card key={step.title} className="relative group hover:border-primary/30 transition-all">
              <div className="absolute top-0 left-0 w-full h-1 bg-white/5 group-hover:bg-primary/30 transition-colors" />
              <div className="pt-4">
                <p className="text-[10px] font-bold text-primary uppercase tracking-[0.2em] mb-2">Phase 0{idx + 1}</p>
                <h3 className="text-sm font-bold text-foreground mb-3 uppercase tracking-tight">{step.title.split('. ')[1]}</h3>
                <p className="text-xs text-muted leading-relaxed italic">{step.text}</p>
              </div>
            </Card>
          ))}
        </div>
      </section>

      <section className="space-y-6">
        <div className="flex items-center gap-3 border-l-2 border-primary pl-4 py-1">
          <Trophy size={20} className="text-primary" />
          <h2 className="text-xl font-bold tracking-widest text-foreground uppercase">Format: Jeder gegen jeden</h2>
        </div>
        <Card>
          <ol className="list-decimal space-y-2 pl-5 text-sm text-muted">
            {LEAGUE_STEPS.map((step) => (
              <li key={step}>{step}</li>
            ))}
          </ol>
        </Card>
      </section>

      {/* Admin Sektion */}
      <section className="space-y-6">
        <div className="flex items-center gap-3 border-l-2 border-primary pl-4 py-1">
          <Settings size={20} className="text-primary" />
          <h2 className="text-xl font-bold tracking-widest text-foreground uppercase">Admin-Aktionen</h2>
        </div>
        <Card className="p-0 overflow-hidden border-white/5">
          <table className="w-full text-sm">
            <thead>
              <tr className="bg-white/5 text-left border-b border-white/10">
                <th className="px-6 py-4 font-bold uppercase tracking-widest text-muted text-[10px]">Aktion</th>
                <th className="px-6 py-4 font-bold uppercase tracking-widest text-muted text-[10px]">Kontext</th>
                <th className="px-6 py-4 font-bold uppercase tracking-widest text-muted text-[10px]">Resultat</th>
              </tr>
            </thead>
            <tbody>
              {ADMIN_ACTIONS.map(([label, when, effect]) => (
                <tr key={label} className="border-b border-white/5 last:border-0 hover:bg-white/[0.02] transition-colors">
                  <td className="px-6 py-4 font-bold text-foreground uppercase tracking-wide text-xs">{label}</td>
                  <td className="px-6 py-4 text-muted text-xs italic">{when}</td>
                  <td className="px-6 py-4 text-primary text-xs font-medium">{effect}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </Card>
      </section>

      <Card className="border-amber-500/20 bg-amber-500/5 p-8 text-center space-y-4">
        <AlertTriangle size={32} className="mx-auto text-amber-500" />
        <h2 className="text-xl font-bold text-foreground uppercase tracking-widest">Probleme im Turnier?</h2>
        <p className="text-sm text-muted max-w-md mx-auto italic">
          Schreib den Moderatoren auf unserem Discord-Server.
        </p>
        <div className="pt-4">
          <a
            href={DISCORD_INVITE_URL}
            target="_blank"
            rel="noopener noreferrer"
            className="inline-flex items-center gap-2 bg-amber-500 hover:bg-amber-600 text-black px-8 py-3 rounded-lg font-bold uppercase tracking-widest text-xs transition-colors"
          >
            Zum Discord
          </a>
        </div>
      </Card>
    </motion.div>
  )
}
