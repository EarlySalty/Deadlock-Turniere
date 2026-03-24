import { useState } from 'react'
import type { FormEvent } from 'react'
import Card from '@/components/ui/Card'
import Button from '@/components/ui/Button'
import { useCreateTournament } from '@/hooks/useTournament'
import type { BracketFormat } from '@/types/tournament'
import { Trophy, AlertCircle, CheckCircle } from 'lucide-react'

export default function CreateTournamentForm() {
  const [name, setName] = useState('')
  const [description, setDescription] = useState('')
  const [teamSize, setTeamSize] = useState(4)
  const [bracketFormat, setBracketFormat] = useState<BracketFormat>('single_elimination')
  const [regStart, setRegStart] = useState('')
  const [regEnd, setRegEnd] = useState('')
  const [successMsg, setSuccessMsg] = useState('')

  const createMutation = useCreateTournament()

  const handleSubmit = (e: FormEvent) => {
    e.preventDefault()
    setSuccessMsg('')

    createMutation.mutate(
      {
        name: name.trim(),
        description: description.trim() || undefined,
        team_size: teamSize,
        bracket_format: bracketFormat,
        registration_start: regStart || undefined,
        registration_end: regEnd || undefined,
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
          {/* Teamgroesse */}
          <div>
            <label htmlFor="team-size" className="block text-sm font-medium text-foreground mb-1.5">
              Teamgroesse
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
            <input
              id="reg-start"
              type="datetime-local"
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
            <input
              id="reg-end"
              type="datetime-local"
              value={regEnd}
              onChange={(e) => setRegEnd(e.target.value)}
              className="w-full bg-background border border-border rounded-lg px-3 py-2 text-foreground focus:outline-none focus:ring-2 focus:ring-primary/50"
            />
          </div>
        </div>

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
