import { type ReactNode } from 'react'
import { Card, CardContent } from '../ui/Card'
import { Skeleton } from '../ui/Skeleton'
import { EmptyState } from './EmptyState'

export interface Column<T = unknown> {
  key: string
  header: string
  width?: string
  render?: (row: T) => ReactNode
}

interface DataTableProps<T> {
  columns: Column<T>[]
  data: T[]
  loading?: boolean
  empty?: ReactNode
  caption?: string
  onRowClick?: (item: T) => void
}

function TableSkeleton({ columns }: { columns: number }) {
  return (
    <Card>
      <CardContent className="p-0">
        <div className="overflow-x-auto">
          <table className="w-full border-collapse text-sm font-body">
            <thead>
              <tr className="bg-muted border-b border-border">
                {Array.from({ length: columns }).map((_, i) => (
                  <th key={i} className="text-left px-4 h-9">
                    <Skeleton className="h-3.5 w-20" />
                  </th>
                ))}
              </tr>
            </thead>
            <tbody>
              {Array.from({ length: 5 }).map((_, rowIdx) => (
                <tr key={rowIdx} className="border-b border-border last:border-b-0">
                  {Array.from({ length: columns }).map((_, colIdx) => (
                    <td key={colIdx} className="px-4 h-9">
                      <Skeleton className="h-3.5 w-24" />
                    </td>
                  ))}
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </CardContent>
    </Card>
  )
}

export function DataTable<T extends Record<string, unknown>>({
  columns,
  data,
  loading = false,
  empty,
  caption,
  onRowClick,
}: DataTableProps<T>) {
  if (loading) {
    return <TableSkeleton columns={columns.length} />
  }

  if (data.length === 0) {
    return <>{empty || <EmptyState title="暂无数据" />}</>
  }

  return (
    <Card>
      <CardContent className="p-0">
        <div className="overflow-x-auto">
          <table className="w-full border-collapse text-sm font-body">
            {caption && <caption className="sr-only">{caption}</caption>}
            <thead>
              <tr className="bg-muted border-b border-border">
                {columns.map((col) => (
                  <th
                    key={col.key}
                    scope="col"
                    className="text-left px-4 h-9 text-xs font-medium text-muted-foreground whitespace-nowrap"
                    style={col.width ? { width: col.width } : undefined}
                  >
                    {col.header}
                  </th>
                ))}
              </tr>
            </thead>
            <tbody>
              {data.map((row, idx) => (
                <tr
                  key={idx}
                  className={`border-b border-border last:border-b-0 hover:bg-muted/50 transition-colors duration-150${onRowClick ? ' cursor-pointer' : ''}`}
                  onClick={onRowClick ? () => onRowClick(row) : undefined}
                >
                  {columns.map((col) => (
                    <td
                      key={col.key}
                      className="px-4 h-9 text-foreground whitespace-nowrap"
                    >
                      {col.render
                        ? col.render(row)
                        : (row[col.key] as ReactNode) ?? '—'}
                    </td>
                  ))}
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </CardContent>
    </Card>
  )
}
