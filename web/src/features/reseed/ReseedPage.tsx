import { useRef, useState } from 'react'
import { useSearchParams } from 'react-router'
import { useVirtualizer } from '@tanstack/react-virtual'
import { useReseedRuns, useReseedRunDetail } from '../../api/hooks/tasks'
import type { DryRunPreviewItem } from '../../api/types'
import { EmptyState, LoadingSkeleton, PageHeader, StatusBadge } from '../../components/shared'
import {
  Badge,
  Button,
  Card,
  CardContent,
  CardHeader,
  CardTitle,
  Spinner,
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from '../../components/ui'
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


function outcomeBadgeVariant(outcome?: string | null): 'success' | 'muted' | 'destructive' | 'info' | 'secondary' {
  switch (outcome) {
    case 'added':
      return 'success'
    case 'skipped':
      return 'muted'
    case 'failed':
      return 'destructive'
    case 'matched':
      return 'info'
    default:
      return 'secondary'
  }
}

function outcomeText(outcome?: string | null): string {
  switch (outcome) {
    case 'added':
      return '已添加'
    case 'skipped':
      return '已跳过'
    case 'failed':
      return '失败'
    case 'matched':
      return '已识别'
    default:
      return outcome ?? '—'
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

  // F5: Use React Query's built-in refetchInterval instead of manual setInterval
  const runsQuery = useReseedRuns({
    taskId,
    limit: 100,
    refetchInterval: (query) => {
      const data = query.state.data
      if (data && data.some((r) => r.status === 'running')) return 1000
      return false
    },
    refetchIntervalInBackground: false,
  })
  const runs = runsQuery.data ?? []

  const detailQuery = useReseedRunDetail(selectedRunId)
  const detail = detailQuery.data

  // F3: Virtualization for the runs table
  const runsParentRef = useRef<HTMLDivElement>(null)
  const runsVirtualizer = useVirtualizer({
    count: runs.length,
    getScrollElement: () => runsParentRef.current,
    estimateSize: () => 40,
  })

  return (
    <div>
      <PageHeader
        title="辅种记录"
        actions={
          taskId ? (
            <span className="text-sm text-muted-foreground">
              筛选：任务 #{taskId}
            </span>
          ) : undefined
        }
      />

      <p className="text-sm text-muted-foreground mb-4">
        查看辅种任务的运行历史和每次运行的详细结果。
      </p>

      {runsQuery.isLoading && <LoadingSkeleton variant="table" rows={5} />}

      {runsQuery.isError && (
        <div className="flex flex-col items-center justify-center py-6 gap-3">
          <p className="text-sm text-destructive m-0">
            加载辅种记录失败：{formatApiError(runsQuery.error, '未知错误')}
          </p>
          <TooltipProvider>
            <Tooltip>
              <TooltipTrigger asChild>
                <Button variant="secondary" size="sm" onClick={() => runsQuery.refetch()}>
                  重试
                </Button>
              </TooltipTrigger>
              <TooltipContent>重新加载辅种记录</TooltipContent>
            </Tooltip>
          </TooltipProvider>
        </div>
      )}

      {!runsQuery.isLoading && !runsQuery.isError && runs.length === 0 && (
        <EmptyState
          title="暂无辅种记录。"
          description="运行辅种任务后，记录将显示在此处。"
        />
      )}

      {!runsQuery.isLoading && !runsQuery.isError && runs.length > 0 && (
        <div className="flex flex-col gap-5">
          {/* Run history table — virtualized */}
          <Card>
            <div className="overflow-x-auto">
              <table className="w-full border-collapse text-sm">
                <caption className="sr-only">辅种运行记录</caption>
                <thead>
                  <tr className="bg-muted border-b border-border">
                    {['任务', '状态', '匹配', '成功', '失败', '跳过', '耗时', '大小', '时间'].map((header) => (
                      <th
                        key={header}
                        scope="col"
                        className="text-left px-4 h-7 text-xs font-medium text-foreground/70 whitespace-nowrap"
                      >
                        {header}
                      </th>
                    ))}
                  </tr>
                </thead>
              </table>
              <div ref={runsParentRef} style={{ height: Math.min(runs.length * 40, 600) + 'px', overflow: 'auto' }}>
                <div style={{ height: runsVirtualizer.getTotalSize(), position: 'relative' }}>
                  {runsVirtualizer.getVirtualItems().map((vItem) => {
                    const run = runs[vItem.index]
                    const isSelected = selectedRunId === run.log_id
                    return (
                      <div
                        key={run.log_id}
                        style={{ position: 'absolute', top: vItem.start, height: vItem.size, width: '100%' }}
                      >
                        <table className="w-full border-collapse text-sm">
                          <tbody>
                            <tr
                              onClick={() => setSelectedRunId(isSelected ? 0 : run.log_id)}
                              onKeyDown={(e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); setSelectedRunId(isSelected ? 0 : run.log_id) } }}
                              tabIndex={0}
                              role="row"
                              aria-label={`辅种记录: ${run.task_name ?? `任务 #${run.task_id}`}`}
                              className={[
                                'border-b border-border cursor-pointer transition-colors duration-150 focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-1 outline-none',
                                isSelected
                                  ? 'bg-card'
                                  : 'hover:bg-muted',
                              ].join(' ')}
                            >
                              <td className="px-4 h-7 text-foreground whitespace-nowrap">
                                {run.task_name ?? `任务 #${run.task_id}`}
                              </td>
                              <td className="px-4 h-7 whitespace-nowrap">
                                <span className="inline-flex items-center gap-1.5">
                                  {run.status === 'running' && <Spinner size="sm" />}
                                  <StatusBadge domain="reseed" status={run.status} />
                                </span>
                              </td>
                              <td className="px-4 h-7 text-foreground whitespace-nowrap">
                                {run.matched_count}
                              </td>
                              <td className="px-4 h-7 text-success whitespace-nowrap">
                                {run.succeeded_count}
                              </td>
                              <td className="px-4 h-7 text-destructive whitespace-nowrap">
                                {run.failed_count}
                              </td>
                              <td className="px-4 h-7 text-muted-foreground whitespace-nowrap">
                                {run.history_skipped_count}
                              </td>
                              <td className="px-4 h-7 text-foreground whitespace-nowrap">
                                {formatDurationMs(run.duration_ms)}
                              </td>
                              <td className="px-4 h-7 text-foreground whitespace-nowrap">
                                {run.total_size != null ? formatBytes(run.total_size) : '—'}
                              </td>
                              <td className="px-4 h-7 text-muted-foreground whitespace-nowrap">
                                {formatShortTime(run.created_at)}
                              </td>
                            </tr>
                          </tbody>
                        </table>
                      </div>
                    )
                  })}
                </div>
              </div>
            </div>
          </Card>

          {/* Detail panel */}
          {selectedRunId > 0 && (
            <Card>
              <CardHeader>
                <CardTitle>运行详情 #{selectedRunId}</CardTitle>
              </CardHeader>
              <CardContent>
                {detailQuery.isLoading && <LoadingSkeleton variant="table" rows={3} />}

                {detailQuery.isError && (
                  <p className="text-sm text-destructive m-0">
                    加载详情失败：{formatApiError(detailQuery.error, '未知错误')}
                  </p>
                )}

                {!detailQuery.isLoading && !detailQuery.isError && detail && (
                  <div className="overflow-x-auto">
                    <table className="w-full border-collapse text-sm">
                      <caption className="sr-only">辅种运行详情</caption>
                      <thead>
                        <tr className="bg-muted border-b border-border">
                          {['站点', '标题', '保存路径', '大小', '结果'].map((header) => (
                            <th
                              key={header}
                              scope="col"
                              className="text-left px-4 h-7 text-xs font-medium text-foreground/70 whitespace-nowrap"
                            >
                              {header}
                            </th>
                          ))}
                        </tr>
                      </thead>
                      <tbody>
                        {detail.items.map((item: DryRunPreviewItem, idx: number) => (
                          <tr
                            key={`${item.pieces_hash}-${idx}`}
                            className="border-b border-border last:border-b-0"
                          >
                            <td className="px-4 h-7 text-foreground whitespace-nowrap">
                              {item.site_name}
                            </td>
                            <td className="px-4 h-7 text-foreground" title={item.title ?? ''}>
                              {truncate(item.title, 60)}
                            </td>
                            <td className="px-4 h-7 text-muted-foreground whitespace-nowrap">
                              {item.save_path}
                            </td>
                            <td className="px-4 h-7 text-foreground whitespace-nowrap">
                              {item.total_size != null ? formatBytes(item.total_size) : '—'}
                            </td>
                            <td className="px-4 h-7 whitespace-nowrap">
                              <Badge variant={outcomeBadgeVariant(item.outcome)}>
                                {outcomeText(item.outcome)}
                              </Badge>
                            </td>
                          </tr>
                        ))}
                        {detail.items.length === 0 && (
                          <tr>
                            <td
                              colSpan={5}
                              className="px-4 py-3 text-center text-muted-foreground"
                            >
                              此次运行无详细条目。
                            </td>
                          </tr>
                        )}
                      </tbody>
                    </table>
                  </div>
                )}
              </CardContent>
            </Card>
          )}
        </div>
      )}
    </div>
  )
}
