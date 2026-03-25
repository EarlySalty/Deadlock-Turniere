import type { InputHTMLAttributes } from 'react'

type DateTimeInputProps = InputHTMLAttributes<HTMLInputElement>

function openNativePicker(target: EventTarget | null) {
  if (!(target instanceof HTMLInputElement)) return
  if (typeof target.showPicker !== 'function') return

  try {
    target.showPicker()
  } catch {
    // Some browsers only allow showPicker during specific user gestures.
  }
}

export default function DateTimeInput({
  className = '',
  onClick,
  onFocus,
  type = 'datetime-local',
  ...props
}: DateTimeInputProps) {
  return (
    <input
      {...props}
      type={type}
      onClick={(event) => {
        onClick?.(event)
        openNativePicker(event.currentTarget)
      }}
      onFocus={(event) => {
        onFocus?.(event)
        openNativePicker(event.currentTarget)
      }}
      className={className}
    />
  )
}
