import { type ReactNode } from 'react'
import { Button } from '../ui/Button'

interface EmptyStateProps {
  icon?: ReactNode
  title: string
  description?: string
  actionLabel?: string
  onAction?: () => void
}

export function EmptyState({ icon, title, description, actionLabel, onAction }: EmptyStateProps) {
  return (
    <div className="flex flex-col items-center justify-center py-[var(--space-10)] px-[var(--space-6)] text-center">
      {icon && (
        <div className="text-[var(--color-text-muted)] mb-[var(--space-4)] text-[24px]">
          {icon}
        </div>
      )}
      <h3 className="text-[var(--text-base)] font-medium text-[var(--color-text)]">
        {title}
      </h3>
      {description && (
        <p className="mt-[var(--space-2)] text-[var(--text-sm)] text-[var(--color-text-muted)] max-w-[280px]">
          {description}
        </p>
      )}
      {actionLabel && onAction && (
        <div className="mt-[var(--space-5)]">
          <Button variant="primary" size="sm" onClick={onAction}>
            {actionLabel}
          </Button>
        </div>
      )}
    </div>
  )
}
