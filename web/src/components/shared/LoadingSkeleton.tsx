import { Skeleton } from '../ui/Skeleton'

type SkeletonVariant = 'text' | 'card' | 'table'

interface LoadingSkeletonProps {
  variant?: SkeletonVariant
  rows?: number
  columns?: number
}

function TextSkeleton({ rows = 3 }: { rows: number }) {
  return (
    <div className="flex flex-col gap-2.5">
      {Array.from({ length: rows }).map((_, i) => (
        <Skeleton
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
      <Skeleton className="h-4 w-1/3 mb-4" />
      <Skeleton className="h-3.5 w-full mb-2" />
      <Skeleton className="h-3.5 w-2/3" />
    </div>
  )
}

const COL_WIDTHS = ['w-24', 'w-32', 'w-20', 'w-16', 'w-28', 'w-24', 'w-20']

function TableSkeleton({ rows = 5, columns = 4 }: { rows: number; columns: number }) {
  return (
    <div className="flex flex-col gap-1">
      {/* Header */}
      <div className="flex gap-4 pb-2 border-b border-border">
        {Array.from({ length: columns }).map((_, i) => (
          <Skeleton key={i} className={`h-3.5 ${COL_WIDTHS[i % COL_WIDTHS.length]}`} />
        ))}
      </div>
      {/* Rows */}
      {Array.from({ length: rows }).map((_, i) => (
        <div key={i} className="flex gap-4 py-2">
          {Array.from({ length: columns }).map((_, j) => (
            <Skeleton key={j} className={`h-3.5 ${COL_WIDTHS[j % COL_WIDTHS.length]}`} />
          ))}
        </div>
      ))}
    </div>
  )
}

export function LoadingSkeleton({ variant = 'text', rows = 3, columns = 4 }: LoadingSkeletonProps) {
  switch (variant) {
    case 'card':
      return <CardSkeleton />
    case 'table':
      return <TableSkeleton rows={rows} columns={columns} />
    default:
      return <TextSkeleton rows={rows} />
  }
}
