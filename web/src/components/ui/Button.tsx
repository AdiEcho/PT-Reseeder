import { type ButtonHTMLAttributes, type ReactNode } from 'react'
import { Spinner } from './Spinner'

type ButtonVariant = 'primary' | 'secondary' | 'danger' | 'ghost'
type ButtonSize = 'sm' | 'md'

interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: ButtonVariant
  size?: ButtonSize
  loading?: boolean
  children: ReactNode
}

const variantClasses: Record<ButtonVariant, string> = {
  primary:
    'bg-[var(--color-accent)] text-[var(--color-accent-text)] hover:bg-[var(--color-accent-hover)]',
  secondary:
    'bg-[var(--color-bg-muted)] text-[var(--color-text)] border border-[var(--color-border)] hover:bg-[var(--color-bg-subtle)]',
  danger:
    'bg-[var(--color-error)] text-white hover:opacity-90',
  ghost:
    'bg-transparent text-[var(--color-text-secondary)] hover:bg-[var(--color-bg-muted)]',
}

const sizeClasses: Record<ButtonSize, string> = {
  sm: 'px-[var(--space-3)] py-[var(--space-1)] text-[var(--text-xs)]',
  md: 'px-[var(--space-4)] py-[var(--space-2)] text-[var(--text-sm)]',
}

export function Button({
  variant = 'primary',
  size = 'md',
  loading = false,
  disabled,
  children,
  className = '',
  ...props
}: ButtonProps) {
  return (
    <button
      disabled={disabled || loading}
      className={[
        'inline-flex items-center justify-center gap-[var(--space-2)]',
        'rounded-[var(--radius-sm)] font-medium leading-tight',
        'transition-all duration-[var(--transition-fast)]',
        'focus:outline-none focus:ring-2 focus:ring-[var(--color-accent)] focus:ring-offset-2',
        'disabled:opacity-50 disabled:cursor-not-allowed',
        'cursor-pointer',
        variantClasses[variant],
        sizeClasses[size],
        className,
      ].join(' ')}
      {...props}
    >
      {loading && <Spinner size="sm" />}
      {children}
    </button>
  )
}
