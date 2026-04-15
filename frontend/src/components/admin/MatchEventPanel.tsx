import { useEffect, useMemo, useState } from 'react'
import Card from '@/components/ui/Card'
import Button from '@/components/ui/Button'
import {
  useApplyMatchConvars,
  useApplyMatchEventPreset,
  useMatchEventPresets,
} from '@/hooks/useTournament'
import type { BracketMatch, MatchEventPreset, Team } from '@/types/tournament'
import {
  AlertCircle,
  CheckCircle2,
  FlaskConical,
  PartyPopper,
  ShieldAlert,
  Swords,
  WandSparkles,
} from 'lucide-react'

interface MatchEventPanelProps {
  tournamentId: number
  matches: BracketMatch[]
  teams: Team[]
  onRefresh?: () => void
}

type PanelMessage = {
  kind: 'success' | 'error'
  text: string
}

function teamName(teamId: number | null, teams: Team[]): string {
  if (teamId === null) return 'TBD'
  return teams.find((team) => team.id === teamId)?.name ?? `Team #${teamId}`
}

function matchLabel(match: BracketMatch, teams: Team[]): string {
  return `${teamName(match.team1_id, teams)} vs ${teamName(match.team2_id, teams)}`
}

function isLiveMatch(match: BracketMatch): boolean {
  return ['lobby_created', 'in_progress'].includes(match.status) && Boolean(match.steam_party_id)
}

function presetConvarPreview(preset: MatchEventPreset): string {
  return Object.entries(preset.convars)
    .slice(0, 3)
    .map(([name, value]) => `${name}=${String(value)}`)
    .join(' | ')
}

export default function MatchEventPanel({
  tournamentId,
  matches,
  teams,
  onRefresh,
}: MatchEventPanelProps) {
  const liveMatches = useMemo(
    () => matches.filter((match) => match.team1_id !== null && match.team2_id !== null),
    [matches],
  )
  const [selectedMatchId, setSelectedMatchId] = useState<number>(() => liveMatches[0]?.id ?? 0)
  const [customRows, setCustomRows] = useState([{ name: '', value: '' }])
  const [message, setMessage] = useState<PanelMessage | null>(null)
  const [activePresetKey, setActivePresetKey] = useState<string | null>(null)

  const selectedMatch = liveMatches.find((match) => match.id === selectedMatchId) ?? liveMatches[0] ?? null
  const selectedMatchLive = selectedMatch ? isLiveMatch(selectedMatch) : false

  useEffect(() => {
    if (!selectedMatch && liveMatches[0]) {
      setSelectedMatchId(liveMatches[0].id)
      return
    }
    if (selectedMatch && !liveMatches.some((match) => match.id === selectedMatch.id)) {
      setSelectedMatchId(liveMatches[0]?.id ?? 0)
    }
  }, [liveMatches, selectedMatch])

  const presetsQuery = useMatchEventPresets(
    tournamentId,
    selectedMatch?.id ?? 0,
    Boolean(selectedMatch),
  )
  const applyPresetMutation = useApplyMatchEventPreset()
  const applyConvarsMutation = useApplyMatchConvars()

  const isBusy = applyPresetMutation.isPending || applyConvarsMutation.isPending

  if (liveMatches.length === 0) {
    return (
      <Card className="p-5">
        <div className="flex items-center gap-2 text-muted">
          <PartyPopper size={18} />
          <span>Keine Bracket-Matches fuer Live-Events verfuegbar.</span>
        </div>
      </Card>
    )
  }

  const resetFeedback = () => setMessage(null)

  const handlePreset = async (presetKey: string, enabled: boolean) => {
    if (!selectedMatch || !selectedMatchLive || isBusy) return
    setActivePresetKey(presetKey)
    resetFeedback()
    try {
      const result = await applyPresetMutation.mutateAsync({
        tournamentId,
        matchId: selectedMatch.id,
        presetKey,
        enabled,
      })
      setMessage({
        kind: 'success',
        text: enabled
          ? `${result.label} wurde auf das Match angewendet.`
          : `${result.label} wurde wieder zurueckgesetzt.`,
      })
      onRefresh?.()
    } catch (error) {
      setMessage({
        kind: 'error',
        text: error instanceof Error ? error.message : 'Preset konnte nicht angewendet werden',
      })
    } finally {
      setActivePresetKey(null)
    }
  }

  const handleCustomApply = async () => {
    if (!selectedMatch || !selectedMatchLive || isBusy) return
    const convars = customRows.reduce<Record<string, string>>((acc, row) => {
      const name = row.name.trim()
      const value = row.value.trim()
      if (!name || !value) return acc
      acc[name] = value
      return acc
    }, {})

    if (Object.keys(convars).length === 0) {
      setMessage({ kind: 'error', text: 'Mindestens eine gueltige ConVar-Zeile ist erforderlich.' })
      return
    }

    resetFeedback()
    try {
      const result = await applyConvarsMutation.mutateAsync({
        tournamentId,
        matchId: selectedMatch.id,
        convars,
      })
      setMessage({
        kind: 'success',
        text: `${Object.keys(result.applied_convars).length} ConVar(s) wurden angewendet.`,
      })
      onRefresh?.()
    } catch (error) {
      setMessage({
        kind: 'error',
        text: error instanceof Error ? error.message : 'ConVars konnten nicht angewendet werden',
      })
    }
  }

  return (
    <div className="space-y-4">
      <Card className="space-y-4 p-5">
        <div className="flex flex-col gap-3 lg:flex-row lg:items-end lg:justify-between">
          <div>
            <div className="flex items-center gap-2 text-sm text-muted">
              <WandSparkles size={16} className="text-primary" />
              <span>Live-Event-Steuerung</span>
            </div>
            <h3 className="mt-1 text-lg font-semibold text-foreground">
              Match auswaehlen und Event direkt aus dem Admin-Panel ausloesen
            </h3>
          </div>

          <div className="min-w-[280px]">
            <label className="mb-2 block text-sm font-medium text-foreground">Aktives Match</label>
            <select
              value={selectedMatch?.id ?? ''}
              onChange={(event) => {
                setSelectedMatchId(Number(event.target.value))
                resetFeedback()
              }}
              className="w-full rounded-lg border border-border bg-background px-3 py-2 text-sm text-foreground focus:border-primary focus:outline-none"
            >
              {liveMatches.map((match) => (
                <option key={match.id} value={match.id}>
                  Match #{match.id} | {matchLabel(match, teams)}
                </option>
              ))}
            </select>
          </div>
        </div>

        {selectedMatch && (
          <div className="grid gap-3 md:grid-cols-3">
            <div className="rounded-xl border border-border bg-background/60 p-4">
              <div className="text-xs uppercase tracking-wider text-muted">Begegnung</div>
              <div className="mt-2 font-semibold text-foreground">{matchLabel(selectedMatch, teams)}</div>
            </div>
            <div className="rounded-xl border border-border bg-background/60 p-4">
              <div className="text-xs uppercase tracking-wider text-muted">Lobby</div>
              <div className="mt-2 font-semibold text-foreground">
                {selectedMatch.party_code || selectedMatch.steam_party_id || 'Noch nicht erstellt'}
              </div>
            </div>
            <div className="rounded-xl border border-border bg-background/60 p-4">
              <div className="text-xs uppercase tracking-wider text-muted">Status</div>
              <div className="mt-2 font-semibold text-foreground">
                {selectedMatchLive ? 'Live steuerbar' : 'Noch keine aktive Lobby'}
              </div>
            </div>
          </div>
        )}

        {!selectedMatchLive && (
          <div className="flex items-center gap-2 rounded-lg border border-amber-500/20 bg-amber-500/10 p-3 text-sm text-amber-300">
            <ShieldAlert size={16} />
            <span>Events greifen erst, wenn fuer das Match eine Lobby erstellt wurde.</span>
          </div>
        )}
      </Card>

      <Card className="space-y-4 p-5">
        <div>
          <h3 className="flex items-center gap-2 text-lg font-semibold text-foreground">
            <FlaskConical size={18} className="text-primary" />
            Presets
          </h3>
          <p className="mt-1 text-sm text-muted">
            Ein Klick fuer lustige Showmatch-Modi. Presets mit Cheat-Hinweis funktionieren nur, wenn die Lobby sie zulaesst.
          </p>
        </div>

        <div className="grid gap-3 lg:grid-cols-2">
          {(presetsQuery.data?.presets ?? []).map((preset) => (
            <div key={preset.key} className="rounded-xl border border-border bg-background/60 p-4">
              <div className="flex items-start justify-between gap-3">
                <div>
                  <h4 className="font-semibold text-foreground">{preset.label}</h4>
                  <p className="mt-1 text-sm text-muted">{preset.description}</p>
                </div>
                {preset.requires_cheats && (
                  <span className="rounded-full border border-amber-500/30 bg-amber-500/10 px-2 py-1 text-[11px] font-medium uppercase tracking-wide text-amber-300">
                    Cheat
                  </span>
                )}
              </div>

              <div className="mt-3 text-xs text-muted">{presetConvarPreview(preset)}</div>

              <div className="mt-4 flex flex-wrap gap-2">
                <Button
                  variant="primary"
                  size="sm"
                  disabled={!selectedMatchLive || isBusy}
                  onClick={() => void handlePreset(preset.key, true)}
                >
                  {activePresetKey === preset.key && applyPresetMutation.isPending ? 'Aktiviert...' : 'Aktivieren'}
                </Button>
                <Button
                  variant="secondary"
                  size="sm"
                  disabled={!selectedMatchLive || isBusy}
                  onClick={() => void handlePreset(preset.key, false)}
                >
                  Zuruecksetzen
                </Button>
              </div>
            </div>
          ))}
        </div>
      </Card>

      <Card className="space-y-4 p-5">
        <div>
          <h3 className="text-lg font-semibold text-foreground">Eigene ConVars</h3>
          <p className="mt-1 text-sm text-muted">
            Fuer Sonderfaelle oder schnelle Tests. Einfach Name und Wert eintragen, ohne extra Steam-Kommandos.
          </p>
        </div>

        <div className="space-y-3">
          {customRows.map((row, index) => (
            <div key={`convar-row-${index}`} className="grid gap-2 md:grid-cols-[1.4fr_1fr_auto]">
              <input
                value={row.name}
                onChange={(event) =>
                  setCustomRows((current) =>
                    current.map((entry, entryIndex) =>
                      entryIndex === index ? { ...entry, name: event.target.value } : entry
                    )
                  )
                }
                placeholder="citadel_dps_multiplier"
                className="rounded-lg border border-border bg-background px-3 py-2 text-sm text-foreground focus:border-primary focus:outline-none"
              />
              <input
                value={row.value}
                onChange={(event) =>
                  setCustomRows((current) =>
                    current.map((entry, entryIndex) =>
                      entryIndex === index ? { ...entry, value: event.target.value } : entry
                    )
                  )
                }
                placeholder="2"
                className="rounded-lg border border-border bg-background px-3 py-2 text-sm text-foreground focus:border-primary focus:outline-none"
              />
              <Button
                type="button"
                variant="ghost"
                size="sm"
                disabled={customRows.length === 1}
                onClick={() =>
                  setCustomRows((current) => current.filter((_, entryIndex) => entryIndex !== index))
                }
              >
                Entfernen
              </Button>
            </div>
          ))}
        </div>

        <div className="flex flex-wrap gap-2">
          <Button
            type="button"
            variant="secondary"
            size="sm"
            onClick={() => setCustomRows((current) => [...current, { name: '', value: '' }])}
          >
            Zeile hinzufuegen
          </Button>
          <Button
            type="button"
            variant="primary"
            size="sm"
            disabled={!selectedMatchLive || isBusy}
            onClick={() => void handleCustomApply()}
          >
            ConVars anwenden
          </Button>
        </div>
      </Card>

      {message && (
        <div
          className={`flex items-center gap-2 rounded-lg p-3 text-sm ${
            message.kind === 'error'
              ? 'border border-red-500/20 bg-red-500/10 text-red-400'
              : 'border border-green-500/20 bg-green-500/10 text-green-400'
          }`}
        >
          {message.kind === 'error' ? <AlertCircle size={16} /> : <CheckCircle2 size={16} />}
          <span>{message.text}</span>
        </div>
      )}

      {selectedMatch && (
        <div className="flex items-center gap-2 rounded-lg border border-border bg-background/60 p-3 text-sm text-muted">
          <Swords size={16} />
          <span>
            Ziel: Match #{selectedMatch.id} | {matchLabel(selectedMatch, teams)}
          </span>
        </div>
      )}
    </div>
  )
}
