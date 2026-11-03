import DateTimeInput from '@/components/ui/DateTimeInput'
import ChoiceCards from './ChoiceCards'
import Field from './Field'
import { INPUT_CLASS, INVITE_MODE_OPTIONS } from './options'
import type { SpecialSettingsState } from './specialSettings'

interface SpecialSettingsFieldsProps {
  idPrefix: string
  value: SpecialSettingsState
  onChange: (value: SpecialSettingsState) => void
}

export default function SpecialSettingsFields({
  idPrefix,
  value,
  onChange,
}: SpecialSettingsFieldsProps) {
  const update = (patch: Partial<SpecialSettingsState>) => onChange({ ...value, ...patch })

  return (
    <div className="space-y-4">
      <ChoiceCards
        name={`${idPrefix}-invite-mode`}
        legend="Teambildung durch Spieler"
        options={INVITE_MODE_OPTIONS}
        value={value.inviteMode}
        onChange={(inviteMode) => update({ inviteMode })}
      />

      {value.inviteMode === 'window' && (
        <div className="grid gap-4 sm:grid-cols-2">
          <Field id={`${idPrefix}-invite-start`} label="Einladungen möglich ab">
            <DateTimeInput
              id={`${idPrefix}-invite-start`}
              value={value.inviteWindowStart}
              onChange={(event) => update({ inviteWindowStart: event.target.value })}
              className={INPUT_CLASS}
            />
          </Field>
          <Field id={`${idPrefix}-invite-end`} label="Einladungen möglich bis">
            <DateTimeInput
              id={`${idPrefix}-invite-end`}
              value={value.inviteWindowEnd}
              onChange={(event) => update({ inviteWindowEnd: event.target.value })}
              className={INPUT_CLASS}
            />
          </Field>
        </div>
      )}

      <div className="grid gap-4 sm:grid-cols-2">
        <Field
          id={`${idPrefix}-no-show-grace`}
          label="No-Show-Frist (Minuten)"
          hint="So lange wartet ein Team auf den Gegner, bevor du das Match kampflos werten kannst."
        >
          <input
            id={`${idPrefix}-no-show-grace`}
            type="number"
            min={1}
            value={value.noShowGrace}
            onChange={(event) => update({ noShowGrace: Math.max(1, Number(event.target.value) || 1) })}
            className={INPUT_CLASS}
          />
        </Field>

        <label className="flex cursor-pointer items-start gap-3 self-start rounded-lg border border-border px-3 py-2.5 sm:mt-7">
          <input
            type="checkbox"
            checked={value.excludeFromLeaderboard}
            onChange={(event) => update({ excludeFromLeaderboard: event.target.checked })}
            className="mt-0.5 h-4 w-4 accent-primary"
          />
          <span>
            <span className="block text-sm font-medium text-foreground">Von Rangliste ausschließen</span>
            <span className="block text-xs text-muted">Ergebnisse zählen nicht für die Rangliste, z. B. bei Testturnieren oder internen Cups.</span>
          </span>
        </label>

      </div>
    </div>
  )
}
