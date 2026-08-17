import { type ReactNode } from 'react'
import { Spinner } from '../ui/Spinner'
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
}

export function DataTable<T extends Record<string, unknown>>({
  columns,
  data,
  loading = false,
  empty,
}: DataTableProps<T>) {
  if (loading) {
    return (
      <div className="flex items-center justify-center py-10">
        <Spinner size="md" />
      </div>
    )
  }

  if (data.length === 0) {
    return <>{empty || <EmptyState title="暂无数据" />}</>
  }

  return (
    <div className="overflow-x-auto border border-border rounded-lg">
      <table className="w-full border-collapse text-sm font-body">
        <thead>
          <tr className="bg-muted border-b border-border">
            {columns.map((col) => (
              <th
                key={col.key}
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
              className="border-b border-border last:border-b-0 hover:bg-muted/50 transition-colors duration-150"
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
  )
}
