import type { DraftAktion } from '@/types/draft'
import { teamFarbe } from './farben'
import RotesX from './RotesX'

function BanKasten({
  team,
  belegt,
  aktiv,
  portraits,
}: {
  team: 1 | 2
  belegt: DraftAktion | undefined
  aktiv: boolean
  portraits: Map<string, string>
}) {
  const farbe = teamFarbe(team)
  const portrait = belegt ? portraits.get(belegt.hero_name) : undefined
  return (
    <div
      className={`relative h-[44px] w-[34px] overflow-hidden rounded-lg border transition-all duration-300 md:h-[58px] md:w-[46px] ${
        aktiv ? 'draft-slot-pulse' : ''
      }`}
      style={{
        backgroundColor: 'rgba(255,255,255,0.02)',
        borderColor: aktiv ? farbe : 'rgba(255,255,255,0.08)',
      }}
    >
      {belegt ? (
        <>
          {portrait && (
            <img
              src={portrait}
              alt={belegt.hero_name}
              className="h-full w-full object-cover opacity-35 grayscale"
            />
          )}
          <RotesX groesse={20} />
          {belegt.is_auto && (
            <span className="absolute left-0.5 top-0.5 rounded bg-black/70 px-1 text-[7px] font-bold uppercase tracking-wider text-white/60">
              Auto
            </span>
          )}
        </>
      ) : aktiv ? (
        <span
          className="absolute inset-0 m-auto h-1.5 w-1.5 rounded-full"
          style={{ backgroundColor: farbe }}
        />
      ) : null}
    </div>
  )
}

export default function BanLeiste({
  bansProTeam,
  banne,
  aktivesTeam,
  istBanZug,
  portraits,
}: {
  bansProTeam: number
  banne: DraftAktion[]
  aktivesTeam: 1 | 2 | null
  istBanZug: boolean
  portraits: Map<string, string>
}) {
  if (bansProTeam === 0) return <div className="h-2" />

  return (
    <div className="flex items-center justify-center gap-4 md:gap-8">
      {([1, 2] as const).map((team) => {
        const meine = banne.filter((b) => b.team_slot === team)
        return (
          <div key={team} className="flex items-center gap-1.5 md:gap-2">
            {team === 2 && (
              <span className="mx-2 text-[9px] uppercase tracking-[0.3em] text-white/30">
                Bans
              </span>
            )}
            {Array.from({ length: bansProTeam }).map((_, i) => (
              <BanKasten
                key={i}
                team={team}
                belegt={meine[i]}
                aktiv={istBanZug && aktivesTeam === team && meine.length === i}
                portraits={portraits}
              />
            ))}
          </div>
        )
      })}
    </div>
  )
}
