import { motion } from 'framer-motion'
import ReactMarkdown from 'react-markdown'
import Card from '@/components/ui/Card'
import {
  BookOpen, Settings, Trophy, Shield,
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

const ADMIN_ACTIONS = [
  ['Anmeldung starten', 'Wenn das Turnier freigegeben werden soll.', 'Setzt den Status auf Anmeldung.'],
  ['Zur Gruppenphase', 'Wenn die Anmeldung beendet ist.', 'Generiert Gruppen und Spiele.'],
  ['Zum Bracket', 'Wenn die Gruppenphase abgeschlossen ist.', 'Erstellt das K.O.-Bracket.'],
  ['Lobby erstellen', 'Sobald ein Bracket-Match bereit ist.', 'Fordert den Steam-Bot an.'],
  ['Ergebnis abrufen', 'Wenn ein Match im Spiel gelaufen ist.', 'Lädt Daten vom Steam-Flow.'],
  ['Ergebnis manuell', 'Wenn Auto-Fetch fehlschlägt.', 'Setzt den Sieger manuell.'],
]

export default function Hilfe() {
  const generalRules = `
# Der Allgemeine Regelwerk

Mit der Teilnahme am Turnier sowie dem abgeschlossenen Check-in bestätigen alle Spieler, die vollständigen Turnierregeln sorgfältig gelesen, verstanden und akzeptiert zu haben. Den Teilnehmern ist bewusst, dass sämtliche Entscheidungen der Turnierleitung auf Grundlage dieser Regeln getroffen werden. 

Jeder Spieler trägt selbst die Verantwortung, sich vor Turnierbeginn über Ablauf, Verhaltensregeln, Teilnahmebedingungen sowie mögliche Sanktionen bei Regelverstößen zu informieren. Unkenntnis der Regeln schützt nicht vor entsprechenden Maßnahmen der Turnierleitung.

---

### 1.0 Allgemeine Regeln

1.1 **Teilnahmepflicht:** Nach erfolgreichem Check-in besteht Teilnahmepflicht. Ein Rückzug oder Nichterscheinen wird als Disqualifikation gewertet.

1.2 **Ausnahmefälle:** Bei Krankheit oder unvorhergesehenen Umständen ist die Turnierleitung frühzeitig zu informieren. Erfolgt keine Rückmeldung und das Match wird nicht angetreten, kann dies zur Disqualifikation führen.

1.3 **Wartezeit:** Die maximale Wartezeit pro Match beträgt **5 Minuten**. Sind beide Spieler früher bereit, kann das Match einvernehmlich auch vor Ablauf der Wartezeit gestartet werden.

1.4 **Termintreue:** Beide Parteien sind verpflichtet, das Match zur angegebenen Uhrzeit auszutragen!

1.5 **Preisgeld:** Das Preisgeld wird freiwillig bereitgestellt. Ein rechtlicher Anspruch auf Auszahlung oder Gewährung des Preisgeldes besteht nicht.

### 2.0 Integrität & Software

2.1 **Cheating:** Jegliche Art von Cheats, Hacks, Scripts oder unfairen Vorteilen ist streng verboten.

2.2 **Exploits:** Das Ausnutzen von Fehlern (Bugs) wird nicht toleriert. Falls ein Spieler beim Ausnutzen eines Bugs überführt wird, hat er mit Strafen von der Turnierleitung zu rechnen bzw. mit einer Disqualifikation!

2.3 **Skins:** Modmanager für Hero Skins sind erlaubt.

### 3.0 Verhalten & Umfeld

3.1 **Respekt:** Respektvoller Umgang wird vorausgesetzt. Rassismus, Diskriminierung, Beleidigungen oder unsportliches Verhalten werden nicht toleriert und können zum Ausschluss vom Turnier führen. Kleiner, respektvoller Banter ist erlaubt, solange Grenzen nicht überschritten werden.

3.2 **Neutralität:** Politische, diskriminierende oder provokative Äußerungen, die den Ablauf oder das Umfeld des Turniers negativ beeinflussen, sind zu unterlassen und können zur sofortigen Disqualifikation führen.

### 4.0 Organisatorisches & Medien

4.1 **Teilnehmerzahl:** Sollte die Mindestanzahl von **12 Teilnehmern** nicht erreicht werden, behält sich die Turnierleitung das Recht vor, das Turnier abzusagen oder zu verschieben!

4.2 **Streaming & Medien:** Die Turniere (insb. „ZeRo´s 1# Keyboard Warriors“) werden live auf Twitch übertragen. Mit dem Check-in erklären sich alle Teilnehmer damit einverstanden, dass ihre Spielinhalte, Ingame-Namen, Charaktere sowie spielbezogene Darstellungen live gestreamt, aufgezeichnet und veröffentlicht werden dürfen.

4.3 **Letzte Instanz:** Die Turnierleitung ist die letzte Entscheidungsinstanz und behält sich vor, in Ausnahmefällen den Turnierverlauf zu beeinflussen und wenn nötig abzuändern.
`

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
        <h1 className="text-4xl font-bold tracking-tighter text-foreground font-display">Der <span className="text-primary">Regelwerk</span></h1>
        <p className="text-muted italic max-w-lg mx-auto">
          "Die universellen Gesetze der Arena. Lerne sie, oder werde Teil der Geschichte."
        </p>
      </div>

      <Card className="p-8 md:p-12 border-white/5 bg-white/[0.02] relative overflow-hidden">
         <div className="absolute top-0 right-0 p-8 opacity-[0.03] pointer-events-none">
            <Shield size={200} />
         </div>
         
         <div className="relative z-10 prose prose-invert prose-amber max-w-none">
            <ReactMarkdown>{generalRules}</ReactMarkdown>
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

      {/* Admin Sektion */}
      <section className="space-y-6">
        <div className="flex items-center gap-3 border-l-2 border-primary pl-4 py-1">
          <Settings size={20} className="text-primary" />
          <h2 className="text-xl font-bold tracking-widest text-foreground uppercase">Admin-Direktiven</h2>
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
        <h2 className="text-xl font-bold text-foreground uppercase tracking-widest">Probleme in der Arena?</h2>
        <p className="text-sm text-muted max-w-md mx-auto italic">
          Kontaktiere die Ältesten (Moderatoren) auf unserem Discord-Server.
        </p>
        <div className="pt-4">
          <a
            href="https://discord.gg/deadlock-de"
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
