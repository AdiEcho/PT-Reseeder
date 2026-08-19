import { useEffect, useRef, useState } from 'react'
import { useVirtualizer } from '@tanstack/react-virtual'
import { useLogFiles, useLogs } from '../../api/hooks/logs'
import type { LogQueryParams } from '../../api/types'
import { EmptyState, LoadingSkeleton, PageHeader } from '../../components/shared'
import {
  Badge,
  Button,
  Card,
  CardContent,
  CardHeader,
  CardTitle,
  Input,
  Select,
} from '../../components/ui'
import { useLogsWs } from '../../ws/useLogsWs'
import { formatLocalTime } from '../../lib/time'

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

function levelBadgeVariant(level: string): 'destructive' | 'warning' | 'info' | 'muted' | 'secondary' {
  switch (level.toUpperCase()) {
    case 'ERROR':
      return 'destructive'
    case 'WARN':
      return 'warning'
    case 'INFO':
      return 'info'
    case 'DEBUG':
      return 'muted'
    case 'TRACE':
      return 'muted'
    default:
      return 'secondary'
  }
}

function levelStreamClass(level: string): string {
  switch (level.toUpperCase()) {
    case 'ERROR':
      return 'text-destructive'
    case 'WARN':
      return 'text-warning'
    case 'INFO':
      return 'text-accent'
    default:
      return 'text-muted-foreground'
  }
}

const LEVEL_OPTIONS = [
  { value: '', label: '所有级别' },
  { value: 'TRACE', label: 'TRACE' },
  { value: 'DEBUG', label: 'DEBUG' },
  { value: 'INFO', label: 'INFO' },
  { value: 'WARN', label: 'WARN' },
  { value: 'ERROR', label: 'ERROR' },
]

const PAGE_SIZE_OPTIONS = [
  { value: '50', label: '50 条' },
  { value: '100', label: '100 条' },
  { value: '200', label: '200 条' },
]

export default function LogsPage() {
  const [filename, setFilename] = useState('')
  const [level, setLevel] = useState('')
  const [keyword, setKeyword] = useState('')
  const [taskId, setTaskId] = useState('')
  const [page, setPage] = useState(1)
  const [pageSize, setPageSize] = useState(50)
  const [streaming, setStreaming] = useState(true)

  const logFilesQuery = useLogFiles()
  const logFiles = logFilesQuery.data ?? []

  const params: LogQueryParams = {
    filename: filename || undefined,
    page,
    page_size: pageSize,
    level: level || undefined,
    keyword: keyword || undefined,
    task_id: taskId ? Number(taskId) : undefined,
  }

  const logsQuery = useLogs(params)
  const logPage = logsQuery.data
  const entries = logPage?.entries ?? []
  const totalPages = logPage ? Math.ceil(logPage.total_lines / logPage.page_size) : 0

  const { lines: wsLines, connected, clear: clearWs } = useLogsWs()

  // F3: Virtualize the real-time log stream
  const streamRef = useRef<HTMLDivElement>(null)
  const streamVirtualizer = useVirtualizer({
    count: wsLines.length,
    getScrollElement: () => streamRef.current,
    estimateSize: () => 24, // line-height matching leading-6
  })

  // Auto-scroll for real-time stream
  useEffect(() => {
    if (streaming && streamRef.current) {
      streamRef.current.scrollTop = streamRef.current.scrollHeight
    }
  }, [wsLines.length, streaming])

  const fileOptions = [
    { value: '', label: '最新日志' },
    ...logFiles.map((f) => ({ value: f.filename, label: `${f.filename} (${formatFileSize(f.size)})` })),
  ]

  return (
    <div>
      <PageHeader title="日志查看器" />

      {/* Filter bar */}
      <Card className="mb-6">
        <CardContent className="p-5">
          <div className="flex flex-col sm:flex-row sm:flex-wrap items-stretch sm:items-end gap-2 sm:gap-4">
            {/* File select */}
            <div className="min-w-[140px]">
              <Select
                label="日志文件"
                value={filename}
                onChange={(e) => { setFilename(e.target.value); setPage(1) }}
                options={fileOptions}
              />
            </div>

            {/* Level select */}
            <div className="min-w-[110px]">
              <Select
                label="级别"
                value={level}
                onChange={(e) => { setLevel(e.target.value); setPage(1) }}
                options={LEVEL_OPTIONS}
              />
            </div>

            {/* Keyword search */}
            <div className="flex-1 min-w-[160px]">
              <Input
                label="关键字"
                type="text"
                placeholder="搜索日志内容..."
                value={keyword}
                onChange={(e) => setKeyword(e.target.value)}
                onKeyDown={(e) => { if (e.key === 'Enter') { setPage(1); logsQuery.refetch() } }}
              />
            </div>

            {/* Task ID */}
            <div className="w-[100px]">
              <Input
                label="任务 ID"
                type="text"
                placeholder="可选"
                value={taskId}
                onChange={(e) => setTaskId(e.target.value)}
                onKeyDown={(e) => { if (e.key === 'Enter') { setPage(1); logsQuery.refetch() } }}
              />
            </div>

            {/* Page size */}
            <div className="w-[90px]">
              <Select
                label="每页"
                value={String(pageSize)}
                onChange={(e) => { setPageSize(Number(e.target.value)); setPage(1) }}
                options={PAGE_SIZE_OPTIONS}
              />
            </div>

            {/* Search button */}
            <Button size="md" onClick={() => { setPage(1); logsQuery.refetch() }}>
              搜索
            </Button>
          </div>
        </CardContent>
      </Card>

      {/* Logs table */}
      {logsQuery.isLoading && <LoadingSkeleton variant="table" rows={8} />}

      {logsQuery.isError && (
        <div className="flex flex-col items-center justify-center py-8 gap-3">
          <p className="text-sm text-destructive m-0">
            加载日志失败：{formatApiError(logsQuery.error, '未知错误')}
          </p>
          <Button variant="secondary" size="sm" onClick={() => logsQuery.refetch()}>
            重试
          </Button>
        </div>
      )}

      {!logsQuery.isLoading && !logsQuery.isError && entries.length === 0 && (
        <EmptyState
          title="未找到日志"
          description="调整筛选条件或等待新日志产生。"
        />
      )}

      {!logsQuery.isLoading && !logsQuery.isError && entries.length > 0 && (
        <>
          <Card>
            <div className="overflow-x-auto">
              <table className="w-full border-collapse text-sm font-body">
                <caption className="sr-only">日志记录</caption>
                <thead>
                  <tr className="bg-muted/60">
                    <th scope="col" className="text-left px-5 py-3 text-xs font-medium text-muted-foreground whitespace-nowrap w-[180px]">
                      时间
                    </th>
                    <th scope="col" className="text-left px-5 py-3 text-xs font-medium text-muted-foreground whitespace-nowrap w-[80px]">
                      级别
                    </th>
                    <th scope="col" className="text-left px-5 py-3 text-xs font-medium text-muted-foreground whitespace-nowrap w-[180px]">
                      目标
                    </th>
                    <th scope="col" className="text-left px-5 py-3 text-xs font-medium text-muted-foreground">
                      消息
                    </th>
                  </tr>
                </thead>
                <tbody>
                  {entries.map((entry, idx) => (
                    <tr
                      key={`${entry.timestamp}-${idx}`}
                      className="border-t border-border hover:bg-muted/30 transition-colors duration-100"
                    >
                      <td className="px-5 py-3 whitespace-nowrap font-mono text-xs text-muted-foreground tabular-nums">
                        {formatLocalTime(entry.timestamp)}
                      </td>
                      <td className="px-5 py-3 whitespace-nowrap">
                        <Badge variant={levelBadgeVariant(entry.level)}>
                          {entry.level}
                        </Badge>
                      </td>
                      <td className="px-5 py-3 whitespace-nowrap text-muted-foreground font-mono text-xs max-w-[180px] truncate">
                        {entry.target}
                      </td>
                      <td className="px-5 py-3 text-foreground break-words max-w-[500px]">
                        {entry.message}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </Card>

          {/* Pagination */}
          <div className="flex items-center justify-between mt-4 py-2 px-1">
            <span className="text-sm text-muted-foreground tabular-nums">
              第 <span className="font-medium text-foreground">{page}</span> / {totalPages} 页
              <span className="ml-2 text-xs text-muted-foreground">（共 {logPage!.total_lines} 条）</span>
            </span>
            <div className="flex items-center gap-2">
              <Button
                variant="secondary"
                size="sm"
                disabled={page <= 1}
                onClick={() => setPage((p) => Math.max(1, p - 1))}
              >
                ← 上一页
              </Button>
              <Button
                variant="secondary"
                size="sm"
                disabled={page >= totalPages}
                onClick={() => setPage((p) => p + 1)}
              >
                下一页 →
              </Button>
            </div>
          </div>
        </>
      )}

      {/* Real-time stream section */}
      <Card className="mt-6">
        <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-3">
          <div className="flex items-center gap-2.5">
            <CardTitle>实时日志流</CardTitle>
            <span className="flex items-center gap-1.5 text-xs text-muted-foreground">
              <span
                className={`inline-block w-1.5 h-1.5 rounded-full ${
                  connected ? 'bg-success' : 'bg-destructive'
                }`}
              />
              {connected ? '已连接' : '已断开'}
            </span>
          </div>
          <div className="flex items-center gap-2">
            <label className="inline-flex items-center gap-1.5 text-xs text-muted-foreground cursor-pointer select-none">
              <input
                type="checkbox"
                checked={streaming}
                onChange={(e) => setStreaming(e.target.checked)}
                className="rounded accent-accent"
              />
              自动滚动
            </label>
            <Button variant="ghost" size="sm" onClick={clearWs}>
              清空
            </Button>
          </div>
        </CardHeader>
        <CardContent className="pt-0">
          <div
            ref={streamRef}
            className="h-52 overflow-y-auto bg-background rounded-lg border border-border px-5 py-3 font-mono text-xs leading-6"
          >
            {wsLines.length === 0 && (
              <EmptyState icon="📡" title="等待日志..." description="实时日志流连接中" />
            )}
            {wsLines.length > 0 && (
              <div style={{ height: streamVirtualizer.getTotalSize(), position: 'relative' }}>
                {streamVirtualizer.getVirtualItems().map((vItem) => {
                  const line = wsLines[vItem.index]
                  return (
                    <div
                      key={vItem.index}
                      style={{ position: 'absolute', top: vItem.start, height: vItem.size, width: '100%' }}
                      className="flex gap-2 hover:bg-muted/30 px-1 rounded"
                    >
                      <span className="text-muted-foreground shrink-0 tabular-nums">
                        {line.timestamp}
                      </span>
                      <span className={`shrink-0 font-semibold w-[50px] ${levelStreamClass(line.level)}`}>
                        {line.level.padEnd(5)}
                      </span>
                      <span className="text-muted-foreground shrink-0 max-w-[200px] truncate">
                        {line.target}
                      </span>
                      <span className="text-foreground">
                        {line.message}
                      </span>
                    </div>
                  )
                })}
              </div>
            )}
          </div>
        </CardContent>
      </Card>
    </div>
  )
}

function formatFileSize(bytes: number): string {
  const kb = 1024
  const mb = kb * 1024
  if (bytes >= mb) return `${(bytes / mb).toFixed(1)} MB`
  if (bytes >= kb) return `${(bytes / kb).toFixed(1)} KB`
  return `${bytes} B`
}
