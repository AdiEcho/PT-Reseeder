import { type InputHTMLAttributes, forwardRef } from 'react'

interface InputProps extends InputHTMLAttributes<HTMLInputElement> {
  label?: string
  error?: string
}

export const Input = forwardRef<HTMLInputElement, InputProps>(
  ({ label, error, className = '', id, ...props }, ref) => {
    const inputId = id || (label ? label.toLowerCase().replace(/\s+/g, '-') : undefined)

    return (
      <div className="flex flex-col gap-[var(--space-1)]">
        {label && (
          <label
            htmlFor={inputId}
            className="text-[var(--text-xs)] text-[var(--color-text-secondary)] font-medium"
          >
            {label}
          </label>
        )}
        <input
          ref={ref}
          id={inputId}
          className={[
            'h-7 px-[var(--space-3)] py-[var(--space-1)]',
            'text-[var(--text-sm)] text-[var(--color-text)]',
            'bg-[var(--color-bg)] border border-[var(--color-border)]',
            'rounded-[var(--radius-sm)]',
            'placeholder:text-[var(--color-text-muted)]',
            'transition-all duration-[var(--transition-fast)]',
            'focus:outline-none focus:ring-2 focus:ring-[var(--color-accent)] focus:ring-offset-2',
            'disabled:opacity-50 disabled:cursor-not-allowed',
            error ? 'border-[var(--color-error)]' : '',
            className,
          ].join(' ')}
          {...props}
        />
        {error && (
          <span className="text-[var(--text-xs)] text-[var(--color-error)]">
            {error}
          </span>
        )}
      </div>
    )
  },
)

Input.displayName = 'Input'
