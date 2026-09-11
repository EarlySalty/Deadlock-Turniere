const FARBEN = ['#c8a86b', '#3b82f6', '#f2eee6']

export default function Konfetti() {
  const stuecke = Array.from({ length: 40 }, (_, i) => ({
    links: ((i * 37 + 13) % 100),
    verz: ((i * 53 + 7) % 120) / 100,
    farbe: FARBEN[i % FARBEN.length],
    breit: 5 + ((i * 29) % 6),
    hoch: 10 + ((i * 17) % 9),
  }))

  return (
    <div className="pointer-events-none absolute inset-0 z-50 overflow-hidden" aria-hidden="true">
      {stuecke.map((s, i) => (
        <span
          key={i}
          className="confetti-stueck"
          style={{
            left: `${s.links}%`,
            width: s.breit,
            height: s.hoch,
            backgroundColor: s.farbe,
            animationDelay: `${s.verz}s`,
          }}
        />
      ))}
    </div>
  )
}
