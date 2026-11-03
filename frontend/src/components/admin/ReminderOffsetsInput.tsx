import { useState } from 'react'
import { Plus, X } from 'lucide-react'
import {
  REMINDER_PRESETS,
  REMINDER_UNITS,
  addOffset,
  formatOffset,
  removeOffset,
  sortOffsets,
  toMinutes,
  type ReminderUnit,
} from './reminderOffsets'

interface ReminderOffsetsInputProps {
  id: string
  label: string
  value: number[]
  onChange: (offsets: number[]) => void
  hint: string
}

const fieldClass =
  'rounded-lg border border-border bg-background px-3 py-2 text-foreground focus:outline-none focus:ring-2 focus:ring-primary/50'

export default function ReminderOffsetsInput({
  id,
  label,
  value,
  onChange,
  hint,
}: ReminderOffsetsInputProps) {
  const [amount, setAmount] = useState('')
  const [unit, setUnit] = useState<ReminderUnit>('hours')
  const customMinutes = toMinutes(Number(amount), unit)
  const offsets = sortOffsets(value)

  const addCustom = () => {
    if (customMinutes === null) return
    onChange(addOffset(offsets, customMinutes))
    setAmount('')
  }

  return (
    <div>
      <label htmlFor={id} className="mb-1.5 block text-sm font-medium text-foreground">
        {label}
      </label>
      <div className="flex flex-wrap gap-1.5">
        {REMINDER_PRESETS.map((preset) => (
          <button
            key={preset.minutes}
            type="button"
            disabled={offsets.includes(preset.minutes)}
            onClick={() => onChange(addOffset(offsets, preset.minutes))}
            className="rounded-md border border-border px-2.5 py-1 text-xs text-foreground hover:border-primary/60 disabled:cursor-default disabled:opacity-40"
          >
            {preset.label}
          </button>
        ))}
      </div>
      <div className="mt-2 flex gap-2">
        <input
          id={id}
          type="number"
          min={1}
          step={1}
          inputMode="numeric"
          value={amount}
          onChange={(event) => setAmount(event.target.value)}
          onKeyDown={(event) => {
            if (event.key !== 'Enter') return
            event.preventDefault()
            addCustom()
          }}
          placeholder="Anzahl"
          className={`${fieldClass} w-24 min-w-0`}
        />
        <select
          aria-label={`${label}: Einheit`}
          value={unit}
          onChange={(event) => setUnit(event.target.value as ReminderUnit)}
          className={`${fieldClass} min-w-0 flex-1`}
        >
          {REMINDER_UNITS.map((option) => (
            <option key={option.value} value={option.value}>
              {option.label}
            </option>
          ))}
        </select>
        <button
          type="button"
          aria-label="Erinnerung hinzufügen"
          disabled={customMinutes === null}
          onClick={addCustom}
          className="rounded-lg border border-border px-3 text-foreground hover:border-primary/60 disabled:cursor-not-allowed disabled:opacity-40"
        >
          <Plus size={16} />
        </button>
      </div>
      <ul className="mt-2 flex flex-wrap gap-1.5" aria-label={`${label}: gewählt`}>
        {offsets.map((minutes) => (
          <li
            key={minutes}
            className="inline-flex items-center gap-1 rounded-full border border-primary/40 bg-primary/10 py-0.5 pl-2.5 pr-1 text-xs text-foreground"
          >
            {formatOffset(minutes)}
            <button
              type="button"
              aria-label={`${formatOffset(minutes)} entfernen`}
              onClick={() => onChange(removeOffset(offsets, minutes))}
              className="rounded-full p-0.5 text-muted hover:text-foreground"
            >
              <X size={12} />
            </button>
          </li>
        ))}
      </ul>
      <p className="mt-1 text-xs text-muted">{hint}</p>
    </div>
  )
}
