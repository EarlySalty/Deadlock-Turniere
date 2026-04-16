import { useState } from 'react'
import { Shield, X } from 'lucide-react'
import Button from '@/components/ui/Button'
import { useSetConsent } from '@/hooks/useTournament'

interface ConsentModalProps {
  onAccepted: () => void
  onDismiss: () => void
}

export default function ConsentModal({ onAccepted, onDismiss }: ConsentModalProps) {
  const [checked, setChecked] = useState(false)
  const setConsentMutation = useSetConsent()

  const handleAccept = () => {
    setConsentMutation.mutate(undefined, {
      onSuccess: () => {
        onAccepted()
      },
    })
  }

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/70 px-4">
      <div className="w-full max-w-lg rounded-xl border border-border bg-card p-6 shadow-2xl">
        <div className="flex items-start justify-between gap-3 mb-5">
          <div className="flex items-center gap-2">
            <Shield size={20} className="text-primary flex-shrink-0" />
            <h2 className="text-lg font-semibold text-foreground">Datenschutz-Einwilligung</h2>
          </div>
          <button
            onClick={onDismiss}
            className="text-muted hover:text-foreground transition-colors"
            aria-label="Schließen"
          >
            <X size={18} />
          </button>
        </div>

        <p className="text-sm text-muted mb-4">
          Bevor du dich für ein Turnier anmelden kannst, benötigen wir deine Einwilligung:
        </p>

        <div className="rounded-lg border border-border bg-background/60 p-4 mb-5 text-sm text-foreground leading-relaxed">
          Ich bin damit einverstanden, dass Turnier-Matches und begleitende Community-Streams live
          übertragen sowie später als Videos, Highlights oder Zusammenschnitte auf Plattformen wie
          Twitch, YouTube oder vergleichbaren Kanälen veröffentlicht werden können. Wenn ich dabei
          im Discord mit anderen spreche oder im Stream sichtbar bin, kann meine Stimme bzw. mein
          Bild im Rahmen dieser Übertragung und Veröffentlichung mit enthalten sein. Eine gezielte
          separate Aufnahme oder bloßstellende Hervorhebung einzelner Personen ist damit nicht
          gemeint.
        </div>

        <label className="flex items-start gap-3 cursor-pointer mb-6">
          <input
            type="checkbox"
            checked={checked}
            onChange={(e) => setChecked(e.target.checked)}
            className="mt-0.5 h-4 w-4 rounded border-border accent-primary"
          />
          <span className="text-sm text-foreground">
            Ich habe die obige Einwilligung gelesen und stimme zu.
          </span>
        </label>

        {setConsentMutation.isError && (
          <div className="mb-4 rounded-lg border border-red-500/20 bg-red-500/10 p-3 text-sm text-red-400">
            Fehler beim Speichern der Einwilligung. Bitte erneut versuchen.
          </div>
        )}

        <div className="flex justify-end gap-2">
          <Button variant="ghost" size="sm" onClick={onDismiss} disabled={setConsentMutation.isPending}>
            Abbrechen
          </Button>
          <Button
            variant="primary"
            size="sm"
            onClick={handleAccept}
            disabled={!checked || setConsentMutation.isPending}
          >
            {setConsentMutation.isPending ? 'Wird gespeichert...' : 'Einwilligen & fortfahren'}
          </Button>
        </div>
      </div>
    </div>
  )
}
