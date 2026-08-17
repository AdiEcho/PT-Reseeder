import { type SelectHTMLAttributes, forwardRef } from 'react'

interface SelectOption {
  value: string
  label: string
}

interface SelectProps extends SelectHTMLAttributes<HTMLSelectElement> {
  label?: string
  options: SelectOption[]
}

export const Select = forwardRef<HTMLSelectElement, SelectProps>(
  ({ label, options, className = '', id, ...props }, ref) => {
    const selectId = id || (label ? label.toLowerCase().replace(/\s+/g, '-') : undefined)

    return (
      <div className="flex flex-col gap-[var(--space-1)]">
        {label && (
          <label
            htmlFor={selectId}
            className="text-[var(--text-xs)] text-[var(--color-text-secondary)] font-medium"
          >
            {label}
          </label>
        )}
        <select
          ref={ref}
          id={selectId}
          className={[
            'h-7 px-[var(--space-3)] py-[var(--space-1)]',
            'text-[var(--text-sm)] text-[var(--color-text)]',
            'bg-[var(--color-bg)] border border-[var(--color-border)]',
            'rounded-[var(--radius-sm)]',
            'transition-all duration-[var(--transition-fast)]',
            'focus:outline-none focus:ring-2 focus:ring-[var(--color-accent)] focus:ring-offset-2',
            'disabled:opacity-50 disabled:cursor-not-allowed',
            'cursor-pointer',
            className,
          ].join(' ')}
          {...props}
        >
          {options.map((opt) => (
            <option key={opt.value} value={opt.value}>
              {opt.label}
            </option>
          ))}
        </select>
      </div>
    )
  },
)

Select.displayName = 'Select'
