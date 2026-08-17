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
      <div className="flex items-center justify-center py-[var(--space-8)]">
        <Spinner size="md" />
      </div>
    )
  }

  if (data.length === 0) {
    return <>{empty || <EmptyState title="No data" />}</>
  }

  return (
    <div className="overflow-x-auto border border-[var(--color-border)] rounded-[var(--radius-md)]">
      <table className="w-full border-collapse text-[var(--text-sm)]">
        <thead>
          <tr className="bg-[var(--color-bg-subtle)] border-b border-[var(--color-border)]">
            {columns.map((col) => (
              <th
                key={col.key}
                className="text-left px-[var(--space-4)] h-7 text-[var(--text-xs)] font-medium text-[var(--color-text-secondary)] whitespace-nowrap"
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
              className="border-b border-[var(--color-border-subtle)] last:border-b-0 hover:bg-[var(--color-bg-subtle)] transition-colors duration-[var(--transition-fast)]"
            >
              {columns.map((col) => (
                <td
                  key={col.key}
                  className="px-[var(--space-4)] h-7 text-[var(--color-text)] whitespace-nowrap"
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
