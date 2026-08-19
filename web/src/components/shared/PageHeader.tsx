import { type ReactNode } from 'react'
import { Separator } from '../ui/Separator'

interface PageHeaderProps {
  title: string
  actions?: ReactNode
}

export function PageHeader({ title, actions }: PageHeaderProps) {
  return (
    <div className="pb-4 mb-6">
      <div className="flex items-center justify-between">
        <h1 className="text-xl font-semibold text-foreground tracking-tight font-body">
          {title}
        </h1>
        {actions && <div className="flex items-center gap-3">{actions}</div>}
      </div>
      <Separator className="mt-4" />
    </div>
  )
}
