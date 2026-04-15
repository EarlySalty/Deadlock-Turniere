import { useState } from 'react'
import type { FormEvent } from 'react'
import Card from '@/components/ui/Card'
import Button from '@/components/ui/Button'
import DateTimeInput from '@/components/ui/DateTimeInput'
import { useCreateTournament } from '@/hooks/useTournament'
import type { BracketFormat, InviteMode, LobbySettingsPreset } from '@/types/tournament'
import { Trophy, AlertCircle, CheckCircle } from 'lucide-react'

const PRESET_LABELS: Record<LobbySettingsPreset, string> = {
  standard: 'Standard',
  fast_mode: 'Fast Mode (schnelle Cooldowns)',
  high_damage: 'High Damage (2× DPS)',
  low_gravity: 'Low Gravity (Mondschwerchkraft)',
  speed_mode: 'Speed Mode (2× Bewegung + schnelle Cooldowns)',
  glass_cannon: 'Glass Cannon (5× Schaden, jeder stirbt sofort)',
  rich_start: 'Rich Start (10.000 Gold beim Start)',
  chaos_mode: 'Chaos Mode (weniger Schwerkraft, schneller, mehr Schaden, viel Gold)',
  all_same_hero: 'All Same Hero (Duplikate erlaubt)',
  immortal: 'Immortal (kein Heldentod)',
  custom: 'Custom (manuelles JSON)',
}

export default function CreateTournamentForm() {
  const [name, setName] = useState('')
  const [description, setDescription] = useState('')
  const [teamSize, setTeamSize] = useState(4)
  const [bracketFormat, setBracketFormat] = useState<BracketFormat>('single_elimination')
  const [regStart, setRegStart] = useState('')
  const [regEnd, setRegEnd] = useState('')
  const [inviteMode, setInviteMode] = useState<InviteMode>('always')
  const [inviteWindowStart, setInviteWindowStart] = useState('')
  const [inviteWindowEnd, setInviteWindowEnd] = useState('')
  const [lobbyPreset, setLobbyPreset] = useState<LobbySettingsPreset>('standard')
  const [customJson, setCustomJson] = useState('')
  const [customJsonError, setCustomJsonError] = useState('')
  const [successMsg, setSuccessMsg] = useState('')

  const createMutation = useCreateTournament()

  const handleSubmit = (e: FormEvent) => {
    e.preventDefault()
    setSuccessMsg('')
    setCustomJsonError('')

    let parsedCustom: Record<string, unknown> | undefined
    if (lobbyPreset === 'custom') {
      try {
        parsedCustom = JSON.parse(customJson)
        if (typeof parsedCustom !== 'object' || Array.isArray(parsedCustom) || parsedCustom === null) {
          setCustomJsonError('Muss ein JSON-Objekt sein, z.B. {"sv_gravity": 200}')
          return
        }
      } catch {
        setCustomJsonError('Ungültiges JSON')
        return
      }
    }

    createMutation.mutate(
      {
        name: name.trim(),
        description: description.trim() || undefined,
        team_size: teamSize,
        bracket_format: bracketFormat,
        registration_start: regStart || undefined,
        registration_end: regEnd || undefined,
        invite_mode: inviteMode,
        invite_window_start: inviteMode === 'window' ? inviteWindowStart || undefined : undefined,
        invite_window_end: inviteMode === 'window' ? inviteWindowEnd || undefined : undefined,
        lobby_settings_preset: lobbyPreset,
        lobby_settings: lobbyPreset === 'custom' ? parsedCustom : undefined,
      },
      {
        onSuccess: (tournament) => {
          setSuccessMsg(`Turnier "${tournament.name}" wurde erfolgreich erstellt!`)
          setName('')
          setDescription('')
          setTeamSize(4)
          setBracketFormat('single_elimination')
          setRegStart('')
          setRegEnd('')
          setInviteMode('always')
          setInviteWindowStart('')
          setInviteWindowEnd('')
          setLobbyPreset('standard')
          setCustomJson('')
        },
      }
    )
  }

  return (
    <Card className="p-6">
      <div className="flex items-center gap-2 mb-6">
        <Trophy size={20} className="text-primary" />
        <h2 className="text-lg font-semibold text-foreground">Neues Turnier erstellen</h2>
      </div>

      <form onSubmit={handleSubmit} className="space-y-5">
        {/* Name */}
        <div>
          <label htmlFor="tournament-name" className="block text-sm font-medium text-foreground mb-1.5">
            Turniername *
          </label>
          <input
            id="tournament-name"
            type="text"
            required
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder="z.B. Deadlock Community Cup #1"
            className="w-full bg-background border border-border rounded-lg px-3 py-2 text-foreground placeholder:text-muted focus:outline-none focus:ring-2 focus:ring-primary/50"
          />
        </div>

        {/* Beschreibung */}
        <div>
          <label htmlFor="tournament-desc" className="block text-sm font-medium text-foreground mb-1.5">
            Beschreibung
          </label>
          <textarea
            id="tournament-desc"
            value={description}
            onChange={(e) => setDescription(e.target.value)}
            rows={3}
            placeholder="Optionale Beschreibung des Turniers..."
            className="w-full bg-background border border-border rounded-lg px-3 py-2 text-foreground placeholder:text-muted focus:outline-none focus:ring-2 focus:ring-primary/50 resize-none"
          />
        </div>

        <div className="grid grid-cols-1 sm:grid-cols-2 gap-4">
          {/* Teamgröße */}
          <div>
            <label htmlFor="team-size" className="block text-sm font-medium text-foreground mb-1.5">
              Teamgröße
            </label>
            <select
              id="team-size"
              value={teamSize}
              onChange={(e) => setTeamSize(Number(e.target.value))}
              className="w-full bg-background border border-border rounded-lg px-3 py-2 text-foreground focus:outline-none focus:ring-2 focus:ring-primary/50"
            >
              {[2, 3, 4, 5, 6].map((n) => (
                <option key={n} value={n}>
                  {n} Spieler
                </option>
              ))}
            </select>
          </div>

          {/* Bracket-Format */}
          <div>
            <label htmlFor="bracket-format" className="block text-sm font-medium text-foreground mb-1.5">
              Bracket-Format
            </label>
            <select
              id="bracket-format"
              value={bracketFormat}
              onChange={(e) => setBracketFormat(e.target.value as BracketFormat)}
              className="w-full bg-background border border-border rounded-lg px-3 py-2 text-foreground focus:outline-none focus:ring-2 focus:ring-primary/50"
            >
              <option value="single_elimination">Single Elimination</option>
              <option value="double_elimination">Double Elimination</option>
            </select>
          </div>
        </div>

        <div className="grid grid-cols-1 sm:grid-cols-2 gap-4">
          {/* Anmeldung Start */}
          <div>
            <label htmlFor="reg-start" className="block text-sm font-medium text-foreground mb-1.5">
              Anmeldung Start
            </label>
            <DateTimeInput
              id="reg-start"
              value={regStart}
              onChange={(e) => setRegStart(e.target.value)}
              className="w-full bg-background border border-border rounded-lg px-3 py-2 text-foreground focus:outline-none focus:ring-2 focus:ring-primary/50"
            />
          </div>

          {/* Anmeldung Ende */}
          <div>
            <label htmlFor="reg-end" className="block text-sm font-medium text-foreground mb-1.5">
              Anmeldung Ende
            </label>
            <DateTimeInput
              id="reg-end"
              value={regEnd}
              onChange={(e) => setRegEnd(e.target.value)}
              className="w-full bg-background border border-border rounded-lg px-3 py-2 text-foreground focus:outline-none focus:ring-2 focus:ring-primary/50"
            />
          </div>
        </div>

        {/* Invite-Modus */}
        <div>
          <label htmlFor="invite-mode" className="block text-sm font-medium text-foreground mb-1.5">
            Einladungs-Modus
          </label>
          <select
            id="invite-mode"
            value={inviteMode}
            onChange={(e) => setInviteMode(e.target.value as InviteMode)}
            className="w-full bg-background border border-border rounded-lg px-3 py-2 text-foreground focus:outline-none focus:ring-2 focus:ring-primary/50"
          >
            <option value="always">Immer erlaubt</option>
            <option value="window">Nur im Zeitfenster</option>
            <option value="never">Deaktiviert (Auto-Balance)</option>
          </select>
          {inviteMode === 'never' && (
            <p className="mt-1 text-xs text-muted">
              Spieler werden beim Check-in-Abschluss automatisch rank-balanced auf Teams verteilt.
            </p>
          )}
        </div>

        {inviteMode === 'window' && (
          <div className="grid grid-cols-1 sm:grid-cols-2 gap-4">
            <div>
              <label htmlFor="invite-start" className="block text-sm font-medium text-foreground mb-1.5">
                Pick-Fenster Start
              </label>
              <DateTimeInput
                id="invite-start"
                value={inviteWindowStart}
                onChange={(e) => setInviteWindowStart(e.target.value)}
                className="w-full bg-background border border-border rounded-lg px-3 py-2 text-foreground focus:outline-none focus:ring-2 focus:ring-primary/50"
              />
            </div>
            <div>
              <label htmlFor="invite-end" className="block text-sm font-medium text-foreground mb-1.5">
                Pick-Fenster Ende
              </label>
              <DateTimeInput
                id="invite-end"
                value={inviteWindowEnd}
                onChange={(e) => setInviteWindowEnd(e.target.value)}
                className="w-full bg-background border border-border rounded-lg px-3 py-2 text-foreground focus:outline-none focus:ring-2 focus:ring-primary/50"
              />
            </div>
          </div>
        )}

        {/* Lobby-Modus */}
        <div>
          <label htmlFor="lobby-preset" className="block text-sm font-medium text-foreground mb-1.5">
            Match-Modus
          </label>
          <select
            id="lobby-preset"
            value={lobbyPreset}
            onChange={(e) => {
              setLobbyPreset(e.target.value as LobbySettingsPreset)
              setCustomJsonError('')
            }}
            className="w-full bg-background border border-border rounded-lg px-3 py-2 text-foreground focus:outline-none focus:ring-2 focus:ring-primary/50"
          >
            {(Object.keys(PRESET_LABELS) as LobbySettingsPreset[]).map((key) => (
              <option key={key} value={key}>{PRESET_LABELS[key]}</option>
            ))}
          </select>
        </div>

        {lobbyPreset === 'custom' && (
          <div>
            <label htmlFor="custom-json" className="block text-sm font-medium text-foreground mb-1.5">
              Custom Convars (JSON)
            </label>
            <textarea
              id="custom-json"
              value={customJson}
              onChange={(e) => { setCustomJson(e.target.value); setCustomJsonError('') }}
              rows={4}
              placeholder={'{"sv_gravity": 400, "citadel_dps_multiplier": 2}'}
              className="w-full bg-background border border-border rounded-lg px-3 py-2 text-foreground placeholder:text-muted font-mono text-sm focus:outline-none focus:ring-2 focus:ring-primary/50 resize-none"
            />
            {customJsonError && (
              <p className="mt-1 text-xs text-red-400">{customJsonError}</p>
            )}
          </div>
        )}

        {/* Fehler */}
        {createMutation.isError && (
          <div className="flex items-center gap-2 text-red-400 text-sm bg-red-500/10 border border-red-500/20 rounded-lg p-3">
            <AlertCircle size={16} />
            <span>
              {createMutation.error instanceof Error
                ? createMutation.error.message
                : 'Fehler beim Erstellen'}
            </span>
          </div>
        )}

        {/* Erfolg */}
        {successMsg && (
          <div className="flex items-center gap-2 text-green-400 text-sm bg-green-500/10 border border-green-500/20 rounded-lg p-3">
            <CheckCircle size={16} />
            <span>{successMsg}</span>
          </div>
        )}

        {/* Submit */}
        <Button
          type="submit"
          variant="primary"
          size="lg"
          disabled={createMutation.isPending || !name.trim()}
          className="w-full"
        >
          {createMutation.isPending ? 'Wird erstellt...' : 'Turnier erstellen'}
        </Button>
      </form>
    </Card>
  )
}
