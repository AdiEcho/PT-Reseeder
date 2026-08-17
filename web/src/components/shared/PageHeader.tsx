import { type ReactNode } from 'react'

interface PageHeaderProps {
  title: string
  actions?: ReactNode
}

export function PageHeader({ title, actions }: PageHeaderProps) {
  return (
    <div className="flex items-center justify-between pb-4 mb-6 border-b border-border">
      <h1 className="text-xl font-semibold text-foreground tracking-tight font-body">
        {title}
      </h1>
      {actions && <div className="flex items-center gap-3">{actions}</div>}
    </div>
  )
}
