import type { ReactNode } from 'react'

interface CardProps {
  children: ReactNode
  className?: string
  hoverable?: boolean
  onClick?: () => void
}

export default function Card({ children, className = '', hoverable = false, onClick }: CardProps) {
  return (
    <div
      className={`glass-card rounded-xl overflow-hidden transition-all duration-300 ${
        hoverable ? 'hover:bg-white/[0.08] hover:border-white/20 hover:-translate-y-0.5 cursor-pointer' : ''
      } ${className}`}
      onClick={onClick}
    >
      <div className="absolute inset-0 bg-gradient-to-br from-white/[0.02] to-transparent pointer-events-none" />
      <div className="relative z-10 p-5">
        {children}
      </div>
    </div>
  )
}
