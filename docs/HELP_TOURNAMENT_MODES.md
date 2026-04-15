# Turnier-Modi Erklärung

## Überblick

Der Turnierbot wählt automatisch zwischen zwei Turnier-Formaten basierend auf der Team-Anzahl. Admins können die Wahl überschreiben.

---

## Automatische Wahl

### 🔄 **Group Stage + Bracket** (>= 12 Teams)
*Das EM/WM-Format*

**Ablauf:**
1. **Gruppen-Phase** (Group Stage)
   - Teams werden in Gruppen aufgeteilt (z.B. 4 Gruppen à 3-4 Teams)
   - Jedes Team spielt gegen alle anderen in seiner Gruppe
   - Jedes Team bekommt Wins/Losses und Punkte
   - Beispiel: 12 Teams → 3 Gruppen á 4 Teams → 6 Matches pro Gruppe

2. **Bracket-Phase** (Playoff)
   - Die Top-Teams aus jeder Gruppe qualifizieren sich
   - Knockout-Turnier: Verlieren = raus (oder Loser's Bracket)
   - Bestimmt den finalen Sieger

**Warum?**
- ✅ Fair: Nicht gleich gegen Favoriten treffen
- ✅ Daten: Viele Ergebnisse für gutes Seeding
- ✅ Professionell: Wie echte Meisterschaften
- ❌ Länger: Viele Matches insgesamt

**Beispiel-Ablauf mit 12 Teams:**
```
Gruppen-Phase:
├─ Gruppe A: Team 1-4 (6 Matches)
├─ Gruppe B: Team 5-8 (6 Matches)
└─ Gruppe C: Team 9-12 (6 Matches)
  = 18 Matches insgesamt

Bracket-Phase (6 Best-of-Gruppen → 6 Teams):
├─ Viertelfinale: 4 Matches
├─ Halbfinale: 2 Matches
└─ Finale: 1 Match
  = 7 Matches insgesamt
```

---

### ⚡ **Nur Bracket** (< 12 Teams)
*Das Direct-Knockout-Format*

**Ablauf:**
1. Teams spielen **direkt** Knockout-Turnier
2. Keine Gruppen-Phase
3. Schneller Abschluss

**Warum?**
- ✅ Schnell: Wenig Matches, schnell vorbei
- ✅ Einfach: Nur Bracket, keine Komplikationen
- ✅ Praktisch: Bei 4-8 Teams sinnvoll
- ❌ Unfair: Erste Runde entscheidend, Glückssache

**Beispiel-Ablauf mit 8 Teams:**
```
Bracket-Phase (direkt):
├─ Viertelfinale: 4 Matches
├─ Halbfinale: 2 Matches
└─ Finale: 1 Match
  = 7 Matches insgesamt
```

---

## Bracket-Formate

Egal welches Mode: Du wählst auch das **Bracket-Format**.

### **Single Elimination**
- **1. Verlust = raus**
- Schnell, einfach, brutal
- Pech im falschen Moment = Turnier vorbei

```
Runde 1:  8 Teams → 4 Gewinner
Runde 2:  4 Teams → 2 Gewinner
Finale:   2 Teams → 1 Sieger
```

### **Double Elimination**
- **2 Verluste = raus**
- Fairer, gibt zweite Chance
- Mehr Matches (Loser's Bracket)
- Sehr komplex, viel Zeit

```
Winners Bracket:
├─ 8 Teams → 4 → 2 → 1 Sieger

Losers Bracket (zweite Chance):
├─ Verlierer des Winners spielen parallel
└─ Loser-Sieger spielt gegen Winners-Sieger im Grand Final

Grand Final:
├─ Winner vs Loser = Turniersieger
```

---

## Admin-Override

Du kannst die automatische Wahl überschreiben:

**Beispiele:**
- **8 Teams, aber du willst Gruppen?** → `force_group_stage: true`
  - 2 Gruppen á 4 Teams + Bracket
  
- **16 Teams, aber nur Bracket?** → `force_bracket_only: true`
  - Direkt Knockout, kein Gruppen-Aufwand

---

## Vergleich: Wann was?

| Szenario | Auto-Wahl | Format | Grund |
|----------|-----------|--------|-------|
| 4 Teams, Casual | ⚡ Bracket | Single Elim | Schnell, einfach |
| 6 Teams, regulär | ⚡ Bracket | Double Elim | Fair, überschaubar |
| 10 Teams, kompetitiv | ⚡ Bracket | Double Elim | Noch machbar ohne Gruppen |
| 12 Teams, EM-Style | 🔄 Group + Bracket | Single Elim | Professional, fair |
| 16 Teams, großes Event | 🔄 Group + Bracket | Double Elim | Maximum fairness |

---

## Zeitplan

### **Bracket-Only (< 12 Teams)**
```
draft → registration → checkin → [direkt Bracket] → completed
```

### **Group + Bracket (>= 12 Teams)**
```
draft → registration → checkin → group_phase_start → [Gruppen spielen]
→ bracket_start → [Bracket spielen] → completed
```

---

## FAQ

**F: Warum 12 Teams Schwelle?**
- A: 12 Teams passen perfekt in 3-4 Gruppen. Darunter nicht sinnvoll.

**F: Kann ich später von Bracket zu Gruppen wechseln?**
- A: Nein. Die Wahl passiert beim Tournament-Create und bestimmt die ganze Struktur.

**F: Was ist besser?**
- A: Kommt drauf an:
  - **Gruppen**: Professionell, fair, aber lang
  - **Bracket**: Schnell, einfach, aber weniger Kontrolle

**F: Brauche ich Double Elimination?**
- A: Nein, Optional:
  - Single Elim = "1 Fehler, raus" (harsh)
  - Double Elim = "2 Fehler, raus" (fairer, aber doppelt so lang)

---

## Kontakt

Fragen? Feedback zur Auto-Logik? Frag den Bot-Admin.
