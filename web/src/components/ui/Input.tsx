import { type InputHTMLAttributes, forwardRef } from 'react'

interface InputProps extends InputHTMLAttributes<HTMLInputElement> {
  label?: string
  error?: string
}

export const Input = forwardRef<HTMLInputElement, InputProps>(
  ({ label, error, className = '', id, ...props }, ref) => {
    const inputId = id || (label ? label.toLowerCase().replace(/\s+/g, '-') : undefined)

    return (
      <div className="flex flex-col gap-1.5">
        {label && (
          <label
            htmlFor={inputId}
            className="text-sm font-medium text-foreground"
          >
            {label}
          </label>
        )}
        <input
          ref={ref}
          id={inputId}
          className={[
            'h-9 px-3 py-1.5',
            'text-sm text-foreground font-body',
            'bg-background border border-border',
            'rounded-md',
            'placeholder:text-muted-foreground',
            'transition-colors duration-150',
            'focus:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-1',
            'disabled:opacity-50 disabled:cursor-not-allowed',
            error ? 'border-destructive' : '',
            className,
          ].join(' ')}
          {...props}
        />
        {error && (
          <span className="text-xs text-destructive">
            {error}
          </span>
        )}
      </div>
    )
  },
)

Input.displayName = 'Input'
