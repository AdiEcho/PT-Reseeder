import { type ReactNode } from 'react'

interface PageHeaderProps {
  title: string
  actions?: ReactNode
}

export function PageHeader({ title, actions }: PageHeaderProps) {
  return (
    <div className="flex items-center justify-between pb-[var(--space-5)] mb-[var(--space-5)] border-b border-[var(--color-border-subtle)]">
      <h1 className="text-[var(--text-xl)] font-semibold text-[var(--color-text)]">
        {title}
      </h1>
      {actions && <div className="flex items-center gap-[var(--space-3)]">{actions}</div>}
    </div>
  )
}
