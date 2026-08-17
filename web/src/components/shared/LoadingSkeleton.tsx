type SkeletonVariant = 'text' | 'card' | 'table'

interface LoadingSkeletonProps {
  variant?: SkeletonVariant
  rows?: number
}

function PulseBar({ className = '' }: { className?: string }) {
  return (
    <div
      className={`animate-pulse bg-muted rounded-md ${className}`}
    />
  )
}

function TextSkeleton({ rows = 3 }: { rows: number }) {
  return (
    <div className="flex flex-col gap-2.5">
      {Array.from({ length: rows }).map((_, i) => (
        <PulseBar
          key={i}
          className={`h-3.5 ${i === rows - 1 ? 'w-3/4' : 'w-full'}`}
        />
      ))}
    </div>
  )
}

function CardSkeleton() {
  return (
    <div className="border border-border rounded-lg p-5">
      <PulseBar className="h-4 w-1/3 mb-4" />
      <PulseBar className="h-3.5 w-full mb-2" />
      <PulseBar className="h-3.5 w-2/3" />
    </div>
  )
}

function TableSkeleton({ rows = 5 }: { rows: number }) {
  return (
    <div className="flex flex-col gap-1">
      {/* Header */}
      <div className="flex gap-4 pb-2 border-b border-border">
        <PulseBar className="h-3.5 w-24" />
        <PulseBar className="h-3.5 w-32" />
        <PulseBar className="h-3.5 w-20" />
        <PulseBar className="h-3.5 w-16" />
      </div>
      {/* Rows */}
      {Array.from({ length: rows }).map((_, i) => (
        <div key={i} className="flex gap-4 py-2">
          <PulseBar className="h-3.5 w-24" />
          <PulseBar className="h-3.5 w-32" />
          <PulseBar className="h-3.5 w-20" />
          <PulseBar className="h-3.5 w-16" />
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
