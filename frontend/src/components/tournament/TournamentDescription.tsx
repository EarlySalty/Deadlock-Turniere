import { useLayoutEffect, useRef, useState } from 'react'

export default function TournamentDescription({ text }: { text: string }) {
  const ref = useRef<HTMLParagraphElement>(null)
  const [open, setOpen] = useState(false)
  const [clipped, setClipped] = useState(false)

  useLayoutEffect(() => {
    const element = ref.current
    if (!element || open) return
    const measure = () => setClipped(element.scrollHeight > element.clientHeight + 1)
    measure()
    const observer = new ResizeObserver(measure)
    observer.observe(element)
    return () => observer.disconnect()
  }, [open, text])

  return (
    <div className="max-w-3xl">
      <p
        ref={ref}
        id="turnier-beschreibung"
        className={`whitespace-pre-line leading-relaxed text-muted ${open ? '' : 'max-h-[4.875rem] overflow-hidden'}`}
      >
        {text}
      </p>
      {(clipped || open) && (
        <button
          type="button"
          onClick={() => setOpen((value) => !value)}
          aria-expanded={open}
          aria-controls="turnier-beschreibung"
          className="mt-2 text-sm font-semibold text-primary underline-offset-4 hover:underline"
        >
          {open ? 'Weniger anzeigen' : 'Mehr anzeigen'}
        </button>
      )}
    </div>
  )
}
