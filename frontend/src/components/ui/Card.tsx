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
      className={`bg-card rounded-xl border border-border p-5 ${hoverable ? 'hover:bg-card-hover transition-colors cursor-pointer' : ''} ${className}`}
      onClick={onClick}
    >
      {children}
    </div>
  )
}
