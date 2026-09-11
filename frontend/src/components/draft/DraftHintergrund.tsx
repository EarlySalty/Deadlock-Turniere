export default function DraftHintergrund({ mitte = false }: { mitte?: boolean }) {
  return (
    <div className="pointer-events-none absolute inset-0 overflow-hidden">
      <div className="absolute left-[15%] top-[20%] h-[500px] w-[500px] rounded-full bg-[#c8a86b]/[0.04] blur-[150px] max-md:h-[250px] max-md:w-[250px]" />
      <div className="absolute right-[5%] top-[5%] h-[500px] w-[500px] rounded-full bg-[#3b82f6]/[0.04] blur-[150px] max-md:h-[250px] max-md:w-[250px]" />
      <div className="absolute bottom-[12%] left-[20%] h-[300px] w-[300px] rounded-full bg-[#c8a86b]/[0.02] blur-[120px] max-md:h-[150px] max-md:w-[150px]" />
      <div className="absolute bottom-[8%] right-[20%] h-[300px] w-[300px] rounded-full bg-[#3b82f6]/[0.02] blur-[120px] max-md:h-[150px] max-md:w-[150px]" />
      <div className="hero-mesh-animated absolute inset-0" />
      {mitte && (
        <>
          <div className="absolute inset-y-0 left-1/2 hidden w-px bg-gradient-to-b from-transparent via-white/10 to-transparent lg:block" />
          <div className="absolute left-1/2 top-1/2 hidden h-32 w-32 -translate-x-1/2 -translate-y-1/2 rounded-full border border-white/[0.06] lg:block" />
        </>
      )}
    </div>
  )
}
