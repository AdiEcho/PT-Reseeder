import { useEffect, useRef, useState } from 'react'
import { useSearchParams } from 'react-router'
import { useReseedRuns, useReseedRunDetail } from '../../api/hooks/tasks'
import type { DryRunPreviewItem } from '../../api/types'
import { EmptyState, LoadingSkeleton, PageHeader } from '../../components/shared'
import { Button } from '../../components/ui'
import { formatBytes, formatDurationMs, formatShortTime } from '../../lib/time'

function formatApiError(err: unknown, fallback: string): string {
  if (!(err instanceof Error) || !err.message) return fallback
  try {
    const parsed = JSON.parse(err.message) as { error?: string }
    if (parsed.error) return parsed.error
  } catch {
    // raw text from the API client
  }
  return err.message
}

function statusLabel(status: string): { text: string; className: string } {
  switch (status) {
    case 'success':
      return { text: '成功', className: 'text-[var(--color-success)]' }
    case 'dry_run':
      return { text: '试运行', className: 'text-[var(--color-accent)]' }
    case 'failed':
      return { text: '失败', className: 'text-[var(--color-error)]' }
    case 'running':
      return { text: '运行中', className: 'text-[var(--color-accent)]' }
    case 'partial':
      return { text: '部分成功', className: 'text-[var(--color-warning)]' }
    case 'skipped':
      return { text: '已跳过', className: 'text-[var(--color-text-muted)]' }
    default:
      return { text: status, className: 'text-[var(--color-text-muted)]' }
  }
}

function outcomeLabel(outcome?: string | null): { text: string; className: string } {
  switch (outcome) {
    case 'added':
      return { text: '已添加', className: 'text-[var(--color-success)]' }
    case 'skipped':
      return { text: '已跳过', className: 'text-[var(--color-text-muted)]' }
    case 'failed':
      return { text: '失败', className: 'text-[var(--color-error)]' }
    case 'matched':
      return { text: '已识别', className: 'text-[var(--color-accent)]' }
    default:
      return { text: outcome ?? '—', className: 'text-[var(--color-text-muted)]' }
  }
}

function truncate(text: string | undefined | null, max: number): string {
  if (!text) return '—'
  return text.length > max ? `${text.slice(0, max)}…` : text
}

export default function ReseedPage() {
  const [searchParams] = useSearchParams()
  const taskIdParam = searchParams.get('task_id')
  const taskId = taskIdParam ? Number(taskIdParam) : undefined

  const [selectedRunId, setSelectedRunId] = useState<number>(0)

  const runsQuery = useReseedRuns({ taskId, limit: 100 })
  const runs = runsQuery.data ?? []
  const shouldPoll = runs.some((r) => r.status === 'running')

  // Poll every 1s when any run is in "running" state
  const pollTimer = useRef<ReturnType<typeof setInterval> | null>(null)
  useEffect(() => {
    if (shouldPoll) {
      pollTimer.current = setInterval(() => {
        runsQuery.refetch()
      }, 1000)
    }
    return () => {
      if (pollTimer.current) {
        clearInterval(pollTimer.current)
        pollTimer.current = null
      }
    }
  }, [shouldPoll, runsQuery])

  const detailQuery = useReseedRunDetail(selectedRunId)
  const detail = detailQuery.data

  return (
    <div>
      <PageHeader
        title="辅种记录"
        actions={
          taskId ? (
            <span className="text-[var(--text-sm)] text-[var(--color-text-muted)]">
              筛选：任务 #{taskId}
            </span>
          ) : undefined
        }
      />

      <p className="text-[var(--text-sm)] text-[var(--color-text-muted)] mb-[var(--space-5)]">
        查看辅种任务的运行历史和每次运行的详细结果。
      </p>

      {runsQuery.isLoading && <LoadingSkeleton variant="table" rows={5} />}

      {runsQuery.isError && (
        <div className="flex flex-col items-center justify-center py-[var(--space-8)] gap-[var(--space-4)]">
          <p className="text-[var(--text-sm)] text-[var(--color-error)] m-0">
            加载辅种记录失败：{formatApiError(runsQuery.error, '未知错误')}
          </p>
          <Button variant="secondary" size="sm" onClick={() => runsQuery.refetch()}>
            重试
          </Button>
        </div>
      )}

      {!runsQuery.isLoading && !runsQuery.isError && runs.length === 0 && (
        <EmptyState
          title="暂无辅种记录。"
          description="运行辅种任务后，记录将显示在此处。"
        />
      )}

      {!runsQuery.isLoading && !runsQuery.isError && runs.length > 0 && (
        <div className="flex flex-col gap-[var(--space-5)]">
          {/* Run history table */}
          <div className="overflow-x-auto border border-[var(--color-border)] rounded-[var(--radius-md)]">
            <table className="w-full border-collapse text-[var(--text-sm)]">
              <thead>
                <tr className="bg-[var(--color-bg-subtle)] border-b border-[var(--color-border)]">
                  {['任务', '状态', '匹配', '成功', '失败', '跳过', '耗时', '大小', '时间'].map((header) => (
                    <th
                      key={header}
                      className="text-left px-[var(--space-4)] h-7 text-[var(--text-xs)] font-medium text-[var(--color-text-secondary)] whitespace-nowrap"
                    >
                      {header}
                    </th>
                  ))}
                </tr>
              </thead>
              <tbody>
                {runs.map((run) => {
                  const status = statusLabel(run.status)
                  const isSelected = selectedRunId === run.log_id
                  return (
                    <tr
                      key={run.log_id}
                      onClick={() => setSelectedRunId(isSelected ? 0 : run.log_id)}
                      className={[
                        'border-b border-[var(--color-border-subtle)] last:border-b-0 cursor-pointer transition-colors duration-[var(--transition-fast)]',
                        isSelected
                          ? 'bg-[var(--color-bg-elevated)]'
                          : 'hover:bg-[var(--color-bg-subtle)]',
                      ].join(' ')}
                    >
                      <td className="px-[var(--space-4)] h-7 text-[var(--color-text)] whitespace-nowrap">
                        {run.task_name ?? `任务 #${run.task_id}`}
                      </td>
                      <td className={`px-[var(--space-4)] h-7 whitespace-nowrap ${status.className}`}>
                        {status.text}
                      </td>
                      <td className="px-[var(--space-4)] h-7 text-[var(--color-text)] whitespace-nowrap">
                        {run.matched_count}
                      </td>
                      <td className="px-[var(--space-4)] h-7 text-[var(--color-success)] whitespace-nowrap">
                        {run.succeeded_count}
                      </td>
                      <td className="px-[var(--space-4)] h-7 text-[var(--color-error)] whitespace-nowrap">
                        {run.failed_count}
                      </td>
                      <td className="px-[var(--space-4)] h-7 text-[var(--color-text-muted)] whitespace-nowrap">
                        {run.history_skipped_count}
                      </td>
                      <td className="px-[var(--space-4)] h-7 text-[var(--color-text)] whitespace-nowrap">
                        {formatDurationMs(run.duration_ms)}
                      </td>
                      <td className="px-[var(--space-4)] h-7 text-[var(--color-text)] whitespace-nowrap">
                        {run.total_size != null ? formatBytes(run.total_size) : '—'}
                      </td>
                      <td className="px-[var(--space-4)] h-7 text-[var(--color-text-muted)] whitespace-nowrap">
                        {formatShortTime(run.created_at)}
                      </td>
                    </tr>
                  )
                })}
              </tbody>
            </table>
          </div>

          {/* Detail panel */}
          {selectedRunId > 0 && (
            <div className="border border-[var(--color-border)] rounded-[var(--radius-md)] p-[var(--space-5)] bg-[var(--color-bg-elevated)]">
              <h3 className="text-[var(--text-base)] font-medium text-[var(--color-text)] m-0 mb-[var(--space-4)]">
                运行详情 #{selectedRunId}
              </h3>

              {detailQuery.isLoading && <LoadingSkeleton variant="table" rows={3} />}

              {detailQuery.isError && (
                <p className="text-[var(--text-sm)] text-[var(--color-error)] m-0">
                  加载详情失败：{formatApiError(detailQuery.error, '未知错误')}
                </p>
              )}

              {!detailQuery.isLoading && !detailQuery.isError && detail && (
                <div className="overflow-x-auto">
                  <table className="w-full border-collapse text-[var(--text-sm)]">
                    <thead>
                      <tr className="bg-[var(--color-bg-subtle)] border-b border-[var(--color-border)]">
                        {['站点', '标题', '保存路径', '大小', '结果'].map((header) => (
                          <th
                            key={header}
                            className="text-left px-[var(--space-4)] h-7 text-[var(--text-xs)] font-medium text-[var(--color-text-secondary)] whitespace-nowrap"
                          >
                            {header}
                          </th>
                        ))}
                      </tr>
                    </thead>
                    <tbody>
                      {detail.items.map((item: DryRunPreviewItem, idx: number) => {
                        const outcome = outcomeLabel(item.outcome)
                        return (
                          <tr
                            key={`${item.pieces_hash}-${idx}`}
                            className="border-b border-[var(--color-border-subtle)] last:border-b-0"
                          >
                            <td className="px-[var(--space-4)] h-7 text-[var(--color-text)] whitespace-nowrap">
                              {item.site_name}
                            </td>
                            <td className="px-[var(--space-4)] h-7 text-[var(--color-text)]" title={item.title ?? ''}>
                              {truncate(item.title, 60)}
                            </td>
                            <td className="px-[var(--space-4)] h-7 text-[var(--color-text-muted)] whitespace-nowrap">
                              {item.save_path}
                            </td>
                            <td className="px-[var(--space-4)] h-7 text-[var(--color-text)] whitespace-nowrap">
                              {item.total_size != null ? formatBytes(item.total_size) : '—'}
                            </td>
                            <td className={`px-[var(--space-4)] h-7 whitespace-nowrap ${outcome.className}`}>
                              {outcome.text}
                            </td>
                          </tr>
                        )
                      })}
                      {detail.items.length === 0 && (
                        <tr>
                          <td
                            colSpan={5}
                            className="px-[var(--space-4)] py-[var(--space-4)] text-center text-[var(--color-text-muted)]"
                          >
                            此次运行无详细条目。
                          </td>
                        </tr>
                      )}
                    </tbody>
                  </table>
                </div>
              )}
            </div>
          )}
        </div>
      )}
    </div>
  )
}
