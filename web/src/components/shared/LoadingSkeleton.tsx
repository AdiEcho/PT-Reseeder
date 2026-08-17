type SkeletonVariant = 'text' | 'card' | 'table'

interface LoadingSkeletonProps {
  variant?: SkeletonVariant
  rows?: number
}

function PulseBar({ className = '' }: { className?: string }) {
  return (
    <div
      className={`animate-pulse bg-[var(--color-bg-muted)] rounded-[var(--radius-sm)] ${className}`}
    />
  )
}

function TextSkeleton({ rows = 3 }: { rows: number }) {
  return (
    <div className="flex flex-col gap-[var(--space-3)]">
      {Array.from({ length: rows }).map((_, i) => (
        <PulseBar
          key={i}
          className={`h-3 ${i === rows - 1 ? 'w-3/4' : 'w-full'}`}
        />
      ))}
    </div>
  )
}

function CardSkeleton() {
  return (
    <div className="border border-[var(--color-border)] rounded-[var(--radius-md)] p-[var(--space-5)]">
      <PulseBar className="h-4 w-1/3 mb-[var(--space-4)]" />
      <PulseBar className="h-3 w-full mb-[var(--space-2)]" />
      <PulseBar className="h-3 w-2/3" />
    </div>
  )
}

function TableSkeleton({ rows = 5 }: { rows: number }) {
  return (
    <div className="flex flex-col gap-[var(--space-1)]">
      {/* Header */}
      <div className="flex gap-[var(--space-4)] pb-[var(--space-2)] border-b border-[var(--color-border)]">
        <PulseBar className="h-3 w-24" />
        <PulseBar className="h-3 w-32" />
        <PulseBar className="h-3 w-20" />
        <PulseBar className="h-3 w-16" />
      </div>
      {/* Rows */}
      {Array.from({ length: rows }).map((_, i) => (
        <div key={i} className="flex gap-[var(--space-4)] py-[var(--space-2)]">
          <PulseBar className="h-3 w-24" />
          <PulseBar className="h-3 w-32" />
          <PulseBar className="h-3 w-20" />
          <PulseBar className="h-3 w-16" />
        </div>
      ))}
    </div>
  )
}

export function LoadingSkeleton({ variant = 'text', rows = 3 }: LoadingSkeletonProps) {
  switch (variant) {
    case 'card':
      return <CardSkeleton />
    case 'table':
      return <TableSkeleton rows={rows} />
    default:
      return <TextSkeleton rows={rows} />
  }
}
