import { useEffect, useRef, useState } from 'react'
import { useLogFiles, useLogs } from '../../api/hooks/logs'
import type { LogQueryParams } from '../../api/types'
import { EmptyState, LoadingSkeleton, PageHeader } from '../../components/shared'
import { Button } from '../../components/ui'
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

function levelBadge(level: string): { className: string } {
  switch (level.toUpperCase()) {
    case 'ERROR':
      return { className: 'bg-destructive/10 text-destructive border-destructive/20' }
    case 'WARN':
      return { className: 'bg-warning/10 text-warning border-warning/20' }
    case 'INFO':
      return { className: 'bg-accent/10 text-accent border-accent/20' }
    case 'DEBUG':
      return { className: 'bg-muted text-muted-foreground border-border' }
    case 'TRACE':
      return { className: 'bg-muted text-muted-foreground border-border' }
    default:
      return { className: 'bg-muted text-foreground border-border' }
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

  // Auto-scroll for real-time stream
  const streamRef = useRef<HTMLDivElement>(null)
  useEffect(() => {
    if (streaming && streamRef.current) {
      streamRef.current.scrollTop = streamRef.current.scrollHeight
    }
  }, [wsLines, streaming])

  const fileOptions = [
    { value: '', label: '最新日志' },
    ...logFiles.map((f) => ({ value: f.filename, label: `${f.filename} (${formatFileSize(f.size)})` })),
  ]

  return (
    <div>
      <PageHeader title="日志查看器" />

      {/* Filter bar — compact inline row */}
      <div className="mb-6 rounded-xl border border-border bg-card/80 backdrop-blur-sm px-6 py-5 shadow-sm">
        <div className="flex flex-wrap items-end gap-4">
          {/* File select */}
          <div className="flex flex-col gap-1 min-w-[140px]">
            <span className="text-xs font-medium text-muted-foreground">日志文件</span>
            <select
              value={filename}
              onChange={(e) => { setFilename(e.target.value); setPage(1) }}
              className="h-9 px-3 text-sm text-foreground bg-background border border-border rounded-lg cursor-pointer transition-colors duration-150 focus:outline-none focus-visible:ring-2 focus-visible:ring-ring"
            >
              {fileOptions.map((opt) => (
                <option key={opt.value} value={opt.value}>{opt.label}</option>
              ))}
            </select>
          </div>

          {/* Level select */}
          <div className="flex flex-col gap-1 min-w-[110px]">
            <span className="text-xs font-medium text-muted-foreground">级别</span>
            <select
              value={level}
              onChange={(e) => { setLevel(e.target.value); setPage(1) }}
              className="h-9 px-3 text-sm text-foreground bg-background border border-border rounded-lg cursor-pointer transition-colors duration-150 focus:outline-none focus-visible:ring-2 focus-visible:ring-ring"
            >
              {LEVEL_OPTIONS.map((opt) => (
                <option key={opt.value} value={opt.value}>{opt.label}</option>
              ))}
            </select>
          </div>

          {/* Keyword search */}
          <div className="flex flex-col gap-1 flex-1 min-w-[160px]">
            <span className="text-xs font-medium text-muted-foreground">关键字</span>
            <input
              type="text"
              placeholder="搜索日志内容..."
              value={keyword}
              onChange={(e) => setKeyword(e.target.value)}
              onKeyDown={(e) => { if (e.key === 'Enter') { setPage(1); logsQuery.refetch() } }}
              className="h-9 px-3 text-sm text-foreground bg-background border border-border rounded-lg transition-colors duration-150 focus:outline-none focus-visible:ring-2 focus-visible:ring-ring placeholder:text-muted-foreground"
            />
          </div>

          {/* Task ID */}
          <div className="flex flex-col gap-1 w-[100px]">
            <span className="text-xs font-medium text-muted-foreground">任务 ID</span>
            <input
              type="text"
              placeholder="可选"
              value={taskId}
              onChange={(e) => setTaskId(e.target.value)}
              onKeyDown={(e) => { if (e.key === 'Enter') { setPage(1); logsQuery.refetch() } }}
              className="h-9 px-3 text-sm text-foreground bg-background border border-border rounded-lg transition-colors duration-150 focus:outline-none focus-visible:ring-2 focus-visible:ring-ring placeholder:text-muted-foreground"
            />
          </div>

          {/* Page size */}
          <div className="flex flex-col gap-1 w-[90px]">
            <span className="text-xs font-medium text-muted-foreground">每页</span>
            <select
              value={String(pageSize)}
              onChange={(e) => { setPageSize(Number(e.target.value)); setPage(1) }}
              className="h-9 px-3 text-sm text-foreground bg-background border border-border rounded-lg cursor-pointer transition-colors duration-150 focus:outline-none focus-visible:ring-2 focus-visible:ring-ring"
            >
              {PAGE_SIZE_OPTIONS.map((opt) => (
                <option key={opt.value} value={opt.value}>{opt.label}</option>
              ))}
            </select>
          </div>

          {/* Search button */}
          <Button size="md" onClick={() => { setPage(1); logsQuery.refetch() }}>
            搜索
          </Button>
        </div>
      </div>

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
          <div className="overflow-x-auto rounded-xl border border-border shadow-sm">
            <table className="w-full border-collapse text-sm font-body">
              <thead>
                <tr className="bg-muted/60">
                  <th className="text-left px-5 py-3 text-xs font-medium text-muted-foreground whitespace-nowrap w-[180px]">
                    时间
                  </th>
                  <th className="text-left px-5 py-3 text-xs font-medium text-muted-foreground whitespace-nowrap w-[80px]">
                    级别
                  </th>
                  <th className="text-left px-5 py-3 text-xs font-medium text-muted-foreground whitespace-nowrap w-[180px]">
                    目标
                  </th>
                  <th className="text-left px-5 py-3 text-xs font-medium text-muted-foreground">
                    消息
                  </th>
                </tr>
              </thead>
              <tbody>
                {entries.map((entry, idx) => {
                  const badge = levelBadge(entry.level)
                  return (
                    <tr
                      key={`${entry.timestamp}-${idx}`}
                      className="border-t border-border hover:bg-muted/30 transition-colors duration-100"
                    >
                      <td className="px-5 py-3 whitespace-nowrap font-mono text-xs text-muted-foreground tabular-nums">
                        {formatLocalTime(entry.timestamp)}
                      </td>
                      <td className="px-5 py-3 whitespace-nowrap">
                        <span className={`inline-flex items-center px-2 py-0.5 text-[11px] font-semibold rounded-md border ${badge.className}`}>
                          {entry.level}
                        </span>
                      </td>
                      <td className="px-5 py-3 whitespace-nowrap text-muted-foreground font-mono text-xs max-w-[180px] truncate">
                        {entry.target}
                      </td>
                      <td className="px-5 py-3 text-foreground break-words max-w-[500px]">
                        {entry.message}
                      </td>
                    </tr>
                  )
                })}
              </tbody>
            </table>
          </div>

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
      <div className="mt-6 rounded-xl border border-border overflow-hidden shadow-sm">
        <div className="flex items-center justify-between px-5 py-3 bg-muted/60 border-b border-border">
          <div className="flex items-center gap-2.5">
            <h3 className="text-sm font-medium text-foreground m-0">
              实时日志流
            </h3>
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
        </div>
        <div
          ref={streamRef}
          className="h-52 overflow-y-auto bg-background px-5 py-3 font-mono text-xs leading-6"
        >
          {wsLines.length === 0 && (
            <p className="text-muted-foreground m-0 text-center py-8">等待日志...</p>
          )}
          {wsLines.map((line, idx) => (
            <div key={idx} className="flex gap-2 hover:bg-muted/30 px-1 rounded">
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
          ))}
        </div>
      </div>
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
