import type { ButtonHTMLAttributes, ReactNode } from 'react'

type ButtonVariant = 'primary' | 'secondary' | 'danger' | 'ghost' | 'outline'
type ButtonSize = 'sm' | 'md' | 'lg'

interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: ButtonVariant
  size?: ButtonSize
  children: ReactNode
}

const variantStyles: Record<ButtonVariant, string> = {
  primary: 'bg-primary hover:bg-primary-hover text-black shadow-[var(--glow-primary)]',
  secondary: 'bg-white/5 hover:bg-white/10 text-foreground border border-white/10 backdrop-blur-sm',
  danger: 'bg-danger/80 hover:bg-danger text-white',
  ghost: 'bg-transparent hover:bg-white/5 text-foreground',
  outline: 'bg-transparent border border-primary/50 text-primary hover:bg-primary/10',
}

const sizeStyles: Record<ButtonSize, string> = {
  sm: 'px-3 py-1.5 text-xs font-bold uppercase tracking-wider',
  md: 'px-5 py-2.5 text-sm font-bold uppercase tracking-wider',
  lg: 'px-8 py-3.5 text-base font-bold uppercase tracking-wider',
}

export default function Button({
  variant = 'primary',
  size = 'md',
  children,
  className = '',
  disabled,
  ...props
}: ButtonProps) {
  return (
    <button
      className={`inline-flex items-center justify-center gap-2 rounded-lg transition-all duration-200 focus:outline-none focus:ring-2 focus:ring-primary/50 ${variantStyles[variant]} ${sizeStyles[size]} ${disabled ? 'opacity-30 cursor-not-allowed grayscale' : 'active:scale-95'} ${className}`}
      disabled={disabled}
      {...props}
    >
      {children}
    </button>
  )
}
