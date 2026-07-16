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
      className={`panel-card rounded-xl transition-all duration-300 ${
        hoverable ? 'hover:bg-card-hover hover:border-border-hover hover:-translate-y-0.5 cursor-pointer' : ''
      } ${className}`}
      onClick={onClick}
    >
      <div className="relative z-10 p-5">
        {children}
      </div>
    </div>
  )
}
