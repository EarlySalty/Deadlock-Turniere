import { useState } from 'react'
import { AlertCircle, CalendarClock, CheckCircle2, Flag, ListRestart } from 'lucide-react'
import Button from '@/components/ui/Button'
import Card from '@/components/ui/Card'
import DateTimeInput from '@/components/ui/DateTimeInput'
import GroupMatchAdminPanel from '@/components/admin/GroupMatchAdminPanel'
import {
  useAdvanceTournament,
  useGenerateGroups,
  useRescheduleLeaguePlan,
} from '@/hooks/useTournament'
import {
  isDecided,
  leagueFinishConfirm,
  leagueRounds,
  localInputToIso,
  slotMinutesOf,
  toLocalInput,
} from '@/lib/league'
import type { Group, Team } from '@/types/tournament'

interface LeaguePlanPanelProps {
  tournamentId: number
  groups: Group[]
  teams: Team[]
  autoLobby: boolean
  countsForLeaderboard: boolean
  onRefresh?: () => void
}

interface ScheduleEditorProps {
  tournamentId: number
  initialStart: string
  initialSlot: number
  canRebuild: boolean
}

type Notice = { kind: 'success' | 'error'; text: string }

function errorText(error: unknown, fallback: string): string {
  return error instanceof Error ? error.message : fallback
}

function NoticeLine({ notice }: { notice: Notice }) {
  const isError = notice.kind === 'error'
  return (
    <div
      className={`flex items-center gap-2 rounded-lg border p-3 text-sm ${
        isError
          ? 'border-red-500/20 bg-red-500/10 text-red-400'
          : 'border-green-500/20 bg-green-500/10 text-green-400'
      }`}
    >
      {isError ? <AlertCircle size={16} /> : <CheckCircle2 size={16} />}
      <span>{notice.text}</span>
    </div>
  )
}

function ScheduleEditor({ tournamentId, initialStart, initialSlot, canRebuild }: ScheduleEditorProps) {
  const reschedule = useRescheduleLeaguePlan()
  const rebuild = useGenerateGroups()
  const [start, setStart] = useState(initialStart)
  const [savedStart, setSavedStart] = useState(initialStart)
  const [slot, setSlot] = useState(initialSlot)
  const [notice, setNotice] = useState<Notice | null>(null)
  const busy = reschedule.isPending || rebuild.isPending
  const startIso = localInputToIso(start)
  const validSlot = Number.isInteger(slot) && slot >= 1 && slot <= 1440
  const startUnsaved = start !== savedStart

  const handleReschedule = () => {
    if (!startIso) return
    reschedule.mutate(
      { tournamentId, start: startIso, slotMinutes: slot },
      {
        onSuccess: () => {
          setSavedStart(start)
          setNotice({ kind: 'success', text: 'Zeitplan wurde angepasst.' })
        },
        onError: (error) =>
          setNotice({ kind: 'error', text: errorText(error, 'Zeitplan konnte nicht geändert werden') }),
      },
    )
  }

  const handleRebuild = () => {
    const question =
      'Spielplan neu erstellen?\n\n' +
      'Alle Spiele des aktuellen Plans werden gelöscht und aus der aktuellen Teamliste neu angelegt. ' +
      'Sind die Teams unverändert, kommen dieselben Paarungen wieder heraus.\n\n' +
      `Runde 1 beginnt zur gespeicherten Startzeit, jede Runde dauert ${slot} Minuten.`
    if (!window.confirm(question)) return
    rebuild.mutate(
      { tournamentId, slotMinutes: slot },
      {
        onSuccess: () => setNotice({ kind: 'success', text: 'Spielplan wurde neu erstellt.' }),
        onError: (error) =>
          setNotice({ kind: 'error', text: errorText(error, 'Spielplan konnte nicht erstellt werden') }),
      },
    )
  }

  return (
    <Card className="space-y-4 p-5">
      <div className="flex items-center gap-2">
        <CalendarClock size={18} className="text-primary" />
        <h3 className="text-base font-semibold text-foreground">Zeitplan</h3>
      </div>
      <div className="grid gap-4 sm:grid-cols-2">
        <div>
          <label htmlFor="league-start" className="mb-1.5 block text-sm font-medium text-foreground">
            Erste Runde beginnt
          </label>
          <DateTimeInput
            id="league-start"
            value={start}
            onChange={(event) => setStart(event.target.value)}
            className="w-full rounded-lg border border-border bg-background px-3 py-2 text-foreground focus:outline-none focus:ring-2 focus:ring-primary/50"
          />
        </div>
        <div>
          <label htmlFor="league-slot" className="mb-1.5 block text-sm font-medium text-foreground">
            Minuten pro Runde
          </label>
          <input
            id="league-slot"
            type="number"
            min={1}
            max={1440}
            value={slot}
            onChange={(event) => setSlot(Number(event.target.value))}
            className="w-full rounded-lg border border-border bg-background px-3 py-2 text-foreground focus:outline-none focus:ring-2 focus:ring-primary/50"
          />
        </div>
      </div>
      <p className="text-xs text-muted">
        Die Paarungen bleiben gleich, nur die Uhrzeiten aller Runden verschieben sich. Bereits
        gewertete Spiele behalten ihr Ergebnis.
      </p>
      <div className="flex flex-wrap gap-2">
        <Button
          variant="primary"
          size="sm"
          disabled={busy || !startIso || !validSlot}
          onClick={handleReschedule}
        >
          <CalendarClock size={14} />
          {reschedule.isPending ? 'Speichert...' : 'Zeiten übernehmen'}
        </Button>
        {canRebuild && (
          <Button
            variant="secondary"
            size="sm"
            disabled={busy || !validSlot || startUnsaved}
            onClick={handleRebuild}
          >
            <ListRestart size={14} />
            {rebuild.isPending ? 'Erstellt...' : 'Spielplan neu erstellen'}
          </Button>
        )}
      </div>
      {canRebuild && startUnsaved && (
        <p className="text-xs text-muted">
          Die geänderte Startzeit ist noch nicht gespeichert. Übernimm sie zuerst mit „Zeiten
          übernehmen“, dann kannst du den Spielplan neu erstellen.
        </p>
      )}
      {notice && <NoticeLine notice={notice} />}
    </Card>
  )
}

interface FinishCardProps {
  tournamentId: number
  openMatches: number
  countsForLeaderboard: boolean
}

function FinishCard({ tournamentId, openMatches, countsForLeaderboard }: FinishCardProps) {
  const advance = useAdvanceTournament()
  const handleFinish = () => {
    if (window.confirm(leagueFinishConfirm(countsForLeaderboard))) {
      advance.mutate(tournamentId)
    }
  }
  return (
    <Card className="space-y-3 p-5">
      <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
        <div>
          <h3 className="text-base font-semibold text-foreground">Turnier abschließen</h3>
          <p className="mt-1 text-sm text-muted">
            {openMatches > 0
              ? `Noch ${openMatches} ${openMatches === 1 ? 'Spiel' : 'Spiele'} offen. Abschließen geht, sobald alle gewertet sind.`
              : 'Alle Spiele sind gewertet. Die Tabelle wird zur Endplatzierung.'}
          </p>
        </div>
        <Button
          variant="primary"
          size="sm"
          disabled={openMatches > 0 || advance.isPending}
          onClick={handleFinish}
        >
          <Flag size={14} />
          {advance.isPending ? 'Schließt ab...' : 'Turnier abschließen'}
        </Button>
      </div>
      {advance.isError && (
        <NoticeLine
          notice={{ kind: 'error', text: errorText(advance.error, 'Abschluss fehlgeschlagen') }}
        />
      )}
    </Card>
  )
}

export default function LeaguePlanPanel({
  tournamentId,
  groups,
  teams,
  autoLobby,
  countsForLeaderboard,
  onRefresh,
}: LeaguePlanPanelProps) {
  const matches = groups.flatMap((group) => group.matches)
  const rounds = leagueRounds(matches)
  const openMatches = matches.filter((match) => !isDecided(match)).length
  const played = matches.some(
    (match) => match.status !== 'pending' || match.winner_id !== null || match.played_at !== null,
  )
  const initialStart = toLocalInput(rounds[0]?.scheduledAt)
  const initialSlot = slotMinutesOf(rounds)

  return (
    <div className="space-y-4">
      <Card className="p-5 text-sm text-muted">
        {autoLobby
          ? 'Lobbys öffnen automatisch, sobald der Termin der aktuellen Runde erreicht ist. Die nächste Runde startet erst, wenn alle Spiele der Runde davor gewertet sind.'
          : 'Die automatische Lobby-Erstellung ist aus. Lobbys der aktuellen Runde legst du unten selbst an.'}{' '}
        Ergebnisse trägst du direkt am Spiel ein und kannst sie jederzeit korrigieren, bis das
        Turnier abgeschlossen ist.
      </Card>
      <ScheduleEditor
        key={`${initialStart}-${initialSlot}`}
        tournamentId={tournamentId}
        initialStart={initialStart}
        initialSlot={initialSlot}
        canRebuild={!played}
      />
      <GroupMatchAdminPanel
        league
        tournamentId={tournamentId}
        groups={groups}
        teams={teams}
        onRefresh={onRefresh}
      />
      <FinishCard
        tournamentId={tournamentId}
        openMatches={openMatches}
        countsForLeaderboard={countsForLeaderboard}
      />
    </div>
  )
}
