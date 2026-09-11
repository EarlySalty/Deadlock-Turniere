import KopierKnopf from './KopierKnopf'
import { motion } from 'framer-motion'
import { raumUrl } from '@/hooks/draftLobbyState'

export default function RaumCode({ code }: { code: string }) {
  return (
    <motion.div
      initial={{ opacity: 0, scale: 0.95 }}
      animate={{ opacity: 1, scale: 1 }}
      transition={{ duration: 0.3, ease: 'easeOut' }}
      className="flex flex-wrap items-center justify-center gap-x-4 gap-y-2 rounded-xl border border-white/[0.08] bg-white/[0.03] px-5 py-3"
    >
      <span className="flex items-baseline gap-3">
        <span className="text-[10px] uppercase tracking-[0.3em] text-white/40">Raum</span>
        <span className="font-mono text-xl font-bold tracking-[0.3em] text-white md:text-2xl">
          {code}
        </span>
      </span>
      <KopierKnopf text={raumUrl(code)} label="Link kopieren" />
    </motion.div>
  )
}
