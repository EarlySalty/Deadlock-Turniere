import { CalendarClock, Radio, Swords } from 'lucide-react'
import { nowState, type NowMatch } from '@/lib/liveNow'
import { teamNameById } from '@/lib/teamName'
import { whenLabel } from '@/lib/whenLabel'
import type { MatchStatus, TeamPublic, TournamentDetailPublic } from '@/types/tournament'

interface LiveNowProps {
  tournament: TournamentDetailPublic
  start: string | null
  onOpenBracket?: () => void
}

const STATUS_LABEL: Partial<Record<MatchStatus, string>> = {
  checkin: 'Check-in',
  lobby_created: 'Lobby offen',
  in_progress: 'Läuft',
}

function MatchLine({ match, teams }: { match: NowMatch; teams: TeamPublic[] }) {
  const when = whenLabel(match.scheduledAt)
  const status = STATUS_LABEL[match.status]
  return (
    <li className="flex min-w-0 flex-col gap-1 rounded-lg border border-border bg-card px-3 py-2.5 sm:flex-row sm:items-center sm:justify-between sm:gap-4">
      <span className="min-w-0 text-sm font-semibold text-foreground">
        <span className="break-words">{teamNameById(teams, match.team1Id ?? 0)}</span>
        <span className="px-1.5 font-normal text-muted">gegen</span>
        <span className="break-words">{teamNameById(teams, match.team2Id ?? 0)}</span>
      </span>
      <span className="flex shrink-0 flex-wrap items-center gap-x-2 text-xs text-muted">
        <span>{match.stage}</span>
        {when && <span>· {when}</span>}
        {status && <span className="font-semibold text-success">· {status}</span>}
      </span>
    </li>
  )
}

export default function LiveNow({ tournament, start, onOpenBracket }: LiveNowProps) {
  const state = nowState(tournament, start)
  if (state.kind === 'none') return null

  if (state.kind === 'start') {
    const checkin = whenLabel(state.checkinStart)
    const begin = whenLabel(state.start)
    return (
      <section aria-label="Turnierstart" className="flex flex-wrap items-center gap-x-6 gap-y-2 rounded-xl border border-border-hover bg-primary-soft px-4 py-3">
        <span className="flex items-center gap-2 text-sm font-semibold text-primary">
          <CalendarClock size={16} aria-hidden="true" />
          Nächster Schritt
        </span>
        {checkin && <span className="text-sm text-foreground">Check-in {checkin}</span>}
        {begin && <span className="text-sm font-semibold text-foreground">Start {begin}</span>}
      </section>
    )
  }

  const live = state.kind === 'live'
  return (
    <section aria-label={live ? 'Läuft gerade' : 'Als Nächstes'} className="space-y-3 rounded-xl border border-border-hover bg-primary-soft p-4">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <h2 className="flex items-center gap-2 text-sm font-semibold text-primary">
          {live ? <Radio size={16} aria-hidden="true" /> : <Swords size={16} aria-hidden="true" />}
          {live ? 'Läuft gerade' : 'Als Nächstes'}
        </h2>
        {onOpenBracket && (
          <button type="button" onClick={onOpenBracket} className="text-sm font-semibold text-primary underline-offset-4 hover:underline">
            Zum Bracket
          </button>
        )}
      </div>
      <ul className="space-y-2">
        {state.matches.map((match) => (
          <MatchLine key={match.key} match={match} teams={tournament.teams} />
        ))}
      </ul>
    </section>
  )
}
