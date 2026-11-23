export default function DraftHintergrund({ mitte = false }: { mitte?: boolean }) {
  return (
    <div className="pointer-events-none absolute inset-0 overflow-hidden" aria-hidden="true">
      <div className="draft-noir-backdrop absolute inset-0" />
      <div className="absolute inset-x-0 top-0 h-px bg-gradient-to-r from-transparent via-[#c8a86b]/35 to-transparent" />
      <div className="absolute left-0 top-[10%] h-[55%] w-[28%] bg-gradient-to-r from-[#be9054]/[0.065] to-transparent" />
      <div className="absolute right-0 top-[10%] h-[55%] w-[28%] bg-gradient-to-l from-[#386d73]/[0.08] to-transparent" />
      <div className="draft-noir-architecture absolute inset-x-0 bottom-0 h-[36%] opacity-40" />
      {mitte && (
        <div className="absolute inset-y-[12%] left-1/2 hidden w-px bg-gradient-to-b from-transparent via-[#c8a86b]/25 to-transparent lg:block" />
      )}
    </div>
  )
}
