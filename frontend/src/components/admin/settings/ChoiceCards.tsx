import type { Choice } from './options'

interface ChoiceCardsProps<T extends string> {
  name: string
  legend: string
  options: Choice<T>[]
  value: T
  onChange: (value: T) => void
  disabled?: boolean
  isOptionDisabled?: (value: T) => boolean
  hint?: string
}

export default function ChoiceCards<T extends string>({
  name,
  legend,
  options,
  value,
  onChange,
  disabled = false,
  isOptionDisabled,
  hint,
}: ChoiceCardsProps<T>) {
  return (
    <fieldset disabled={disabled} className="disabled:opacity-60">
      <legend className="mb-1.5 text-sm font-medium text-foreground">{legend}</legend>
      <div className="grid gap-2 sm:grid-cols-2">
        {options.map((option) => {
          const selected = option.value === value
          return (
            <label
              key={option.value}
              className={`flex cursor-pointer items-start gap-3 rounded-lg border px-3 py-2.5 transition-colors has-[:disabled]:cursor-not-allowed ${
                selected ? 'border-primary/60 bg-primary/10' : 'border-border hover:bg-card-hover'
              }`}
            >
              <input
                type="radio"
                name={name}
                value={option.value}
                checked={selected}
                disabled={isOptionDisabled?.(option.value)}
                onChange={() => onChange(option.value)}
                className="mt-1 h-4 w-4 shrink-0 accent-primary"
              />
              <span>
                <span className="block text-sm font-medium text-foreground">{option.label}</span>
                <span className="block text-xs text-muted">{option.description}</span>
              </span>
            </label>
          )
        })}
      </div>
      {hint && <p className="mt-1.5 text-xs text-muted">{hint}</p>}
    </fieldset>
  )
}
