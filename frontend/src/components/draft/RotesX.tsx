export default function RotesX({ groesse = 18, strich = 3.5 }: { groesse?: number; strich?: number }) {
  return (
    <svg
      width={groesse}
      height={groesse}
      viewBox="0 0 24 24"
      fill="none"
      className="absolute inset-0 m-auto"
      aria-hidden="true"
    >
      <path d="M5 5 L19 19" stroke="#ef4444" strokeWidth={strich} strokeLinecap="round" />
      <path d="M19 5 L5 19" stroke="#ef4444" strokeWidth={strich} strokeLinecap="round" />
    </svg>
  )
}
