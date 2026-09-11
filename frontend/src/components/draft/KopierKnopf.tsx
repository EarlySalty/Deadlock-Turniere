import { useState } from 'react'
import { motion } from 'framer-motion'
import { Check, Copy } from 'lucide-react'

export default function KopierKnopf({
  text,
  label,
  variante = 'dunkel',
  pulse = false,
}: {
  text: string
  label: string
  variante?: 'gold' | 'dunkel'
  pulse?: boolean
}) {
  const [kopiert, setKopiert] = useState(false)

  const kopieren = async () => {
    try {
      await navigator.clipboard.writeText(text)
      setKopiert(true)
      window.setTimeout(() => setKopiert(false), 1600)
    } catch {
      setKopiert(false)
    }
  }

  const stil =
    variante === 'gold'
      ? 'bg-[#c8a86b] text-[#0b0b0b]'
      : 'border border-white/10 bg-white/[0.04] text-white/60 hover:text-white hover:border-white/25'

  return (
    <button
      type="button"
      onClick={kopieren}
      className={`inline-flex shrink-0 items-center gap-1.5 rounded-lg px-3 py-1.5 text-[10px] font-bold uppercase tracking-[0.15em] transition-colors ${stil} ${pulse && !kopiert ? 'cta-pulse-gold' : ''}`}
    >
      {kopiert ? (
        <motion.span
          key="haken"
          initial={{ scale: 0 }}
          animate={{ scale: 1 }}
          transition={{ type: 'spring', stiffness: 200 }}
          className="inline-flex"
        >
          <Check size={13} />
        </motion.span>
      ) : (
        <Copy size={13} />
      )}
      <span>{kopiert ? 'Kopiert' : label}</span>
    </button>
  )
}
