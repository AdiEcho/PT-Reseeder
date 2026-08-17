import { useMemo, useState } from 'react'
import { useDashboard } from '../../api/hooks'
import type {
  DashboardOverview,
  SiteReseedStats,
  SiteUserInfo,
  TrendPoint,
  UserInfoAggregate,
} from '../../api/types'
import { EmptyState, LoadingSkeleton, PageHeader } from '../../components/shared'
import { formatBytes, formatDuration, formatShortTime } from '../../lib/time'
import { useDashboardWs } from '../../ws/useDashboardWs'

const DAY_OPTIONS = [
  { value: 7, label: '7天' },
  { value: 30, label: '30天' },
  { value: 0, label: '全部' },
] as const

export function DashboardPage() {
  const [days, setDays] = useState(7)
  const { data, isLoading, isError, error, refetch } = useDashboard(days)
  useDashboardWs()

  const showError = isError && !data
  const showLoading = isLoading && !data

  return (
    <div>
      <PageHeader
        title="仪表盘"
        actions={<DaysSelector days={days} onChange={setDays} />}
      />

      {showLoading ? (
        <div className="flex flex-col gap-5">
          <div className="grid grid-cols-2 sm:grid-cols-3 xl:grid-cols-5 gap-4">
            {Array.from({ length: 5 }).map((_, i) => (
              <LoadingSkeleton key={i} variant="card" />
            ))}
          </div>
          <LoadingSkeleton variant="table" rows={6} />
        </div>
      ) : showError ? (
        <EmptyState
          title="仪表盘加载失败"
          description={error instanceof Error ? error.message : '无法获取仪表盘数据'}
          actionLabel="重试"
          onAction={() => {
            void refetch()
          }}
        />
      ) : data ? (
        <div className="flex flex-col gap-5">
          <OverviewCards overview={data.overview} />
          <TrendChart points={data.trend} />
          <SiteStatsTable stats={data.site_stats} />
          <UserInfoSection info={data.user_info} />
        </div>
      ) : null}
    </div>
  )
}

function DaysSelector({
  days,
  onChange,
}: {
  days: number
  onChange: (days: number) => void
}) {
  return (
    <div className="inline-flex items-center gap-0.5 p-0.5 rounded-md border border-border bg-muted">
      {DAY_OPTIONS.map((opt) => {
        const active = days === opt.value
        return (
          <button
            key={opt.value}
            type="button"
            onClick={() => onChange(opt.value)}
            className={[
              'px-3 py-1 rounded text-sm font-medium cursor-pointer border transition-colors duration-150',
              active
                ? 'bg-card text-foreground border-border shadow-sm'
                : 'bg-transparent text-muted-foreground border-transparent hover:text-foreground',
            ].join(' ')}
          >
            {opt.label}
          </button>
        )
      })}
    </div>
  )
}

function OverviewCards({ overview }: { overview: DashboardOverview }) {
  return (
    <div className="grid grid-cols-2 sm:grid-cols-3 xl:grid-cols-5 gap-4">
      <StatCard label="运行中任务" value={String(overview.running_tasks)} accent="accent" />
      <StatCard label="今日成功" value={String(overview.today_success)} accent="success" />
      <StatCard label="今日失败" value={String(overview.today_failed)} accent="destructive" />
      <StatCard label="站点数" value={String(overview.total_sites)} accent="accent" />
      <StatCard label="监控种子数" value={String(overview.tracked_torrents)} accent="accent" />
    </div>
  )
}

function StatCard({
  label,
  value,
  accent,
}: {
  label: string
  value: string
  accent: 'accent' | 'success' | 'destructive' | 'warning'
}) {
  const accentColors: Record<string, string> = {
    accent: 'border-l-accent',
    success: 'border-l-success',
    destructive: 'border-l-destructive',
    warning: 'border-l-warning',
  }
  return (
    <div
      className={`rounded-lg border border-border bg-card px-5 py-4 border-l-4 ${accentColors[accent]}`}
    >
      <div className="text-2xl font-bold tabular-nums leading-tight text-foreground">
        {value}
      </div>
      <div className="mt-0.5 text-xs uppercase tracking-wide text-muted-foreground">
        {label}
      </div>
    </div>
  )
}

function TrendChart({ points }: { points: TrendPoint[] }) {
  const chart = useMemo(() => buildTrendPaths(points), [points])

  return (
    <section className="rounded-lg border border-border bg-card p-5">
      <h2 className="mb-4 text-base font-semibold text-foreground">
        辅种趋势
      </h2>
      {points.length === 0 || !chart ? (
        <EmptyState title="暂无趋势数据。" />
      ) : (
        <svg
          viewBox={`0 0 ${chart.width} ${chart.height}`}
          className="block w-full h-auto max-h-[220px]"
          preserveAspectRatio="xMidYMid meet"
        >
          <line
            x1={chart.padding}
            y1={chart.padding}
            x2={chart.padding}
            y2={chart.padding + chart.chartH}
            className="stroke-border"
            strokeWidth="1"
          />
          <line
            x1={chart.padding}
            y1={chart.padding + chart.chartH}
            x2={chart.padding + chart.chartW}
            y2={chart.padding + chart.chartH}
            className="stroke-border"
            strokeWidth="1"
          />
          <polyline
            points={chart.successPoints}
            fill="none"
            className="stroke-success"
            strokeWidth="2"
          />
          <polyline
            points={chart.failedPoints}
            fill="none"
            className="stroke-destructive"
            strokeWidth="2"
          />
          {chart.xLabels.map((label) => (
            <text
              key={`${label.x}-${label.text}`}
              x={label.x}
              y={chart.padding + chart.chartH + 16}
              textAnchor="middle"
              fontSize="11"
              className="fill-muted-foreground"
            >
              {label.text}
            </text>
          ))}
          <circle
            cx={chart.padding + 10}
            cy="12"
            r="4"
            className="fill-success"
          />
          <text
            x={chart.padding + 18}
            y="16"
            fontSize="12"
            className="fill-muted-foreground"
          >
            成功
          </text>
          <circle
            cx={chart.padding + 80}
            cy="12"
            r="4"
            className="fill-destructive"
          />
          <text
            x={chart.padding + 88}
            y="16"
            fontSize="12"
            className="fill-muted-foreground"
          >
            失败
          </text>
        </svg>
      )}
    </section>
  )
}

function buildTrendPaths(points: TrendPoint[]) {
  if (points.length === 0) return null

  const maxVal = Math.max(1, ...points.map((p) => Math.max(p.succeeded, p.failed)))
  const width = 800
  const height = 200
  const padding = 40
  const chartW = width - padding * 2
  const chartH = height - padding * 2
  const n = points.length
  const step = n > 1 ? chartW / (n - 1) : chartW

  const toPoint = (i: number, value: number) => {
    const x = padding + i * step
    const y = padding + chartH - (value / maxVal) * chartH
    return `${x.toFixed(1)},${y.toFixed(1)}`
  }

  const successPoints = points.map((p, i) => toPoint(i, p.succeeded)).join(' ')
  const failedPoints = points.map((p, i) => toPoint(i, p.failed)).join(' ')

  const labelStride = Math.max(1, Math.floor(n / 7))
  const xLabels = points
    .map((p, i) => ({ p, i }))
    .filter(({ i }) => (n <= 10 ? true : i % labelStride === 0 || i === n - 1))
    .map(({ p, i }) => ({
      x: Number((padding + i * step).toFixed(1)),
      text: p.date.length >= 10 ? p.date.slice(5, 10) : p.date,
    }))

  return { width, height, padding, chartW, chartH, successPoints, failedPoints, xLabels }
}

function SiteStatsTable({ stats }: { stats: SiteReseedStats[] }) {
  return (
    <section className="rounded-lg border border-border bg-card p-5">
      <h2 className="mb-4 text-base font-semibold text-foreground">
        站点辅种统计
      </h2>
      {stats.length === 0 ? (
        <EmptyState title="暂无辅种记录。" />
      ) : (
        <div className="overflow-x-auto border border-border rounded-lg">
          <table className="w-full border-collapse text-sm font-body">
            <thead>
              <tr className="bg-muted border-b border-border">
                {['站点', '匹配数', '成功', '失败', '跳过', '成功率', '状态'].map((h) => (
                  <th
                    key={h}
                    className="text-left px-4 h-9 text-xs font-medium text-muted-foreground whitespace-nowrap"
                  >
                    {h}
                  </th>
                ))}
              </tr>
            </thead>
            <tbody>
              {stats.map((s) => {
                const status = breakerLabel(s.breaker_status)
                return (
                  <tr
                    key={s.site_id}
                    className="border-b border-border last:border-b-0 hover:bg-muted/50 transition-colors duration-150"
                  >
                    <td className="px-4 h-9 whitespace-nowrap">{s.site_name}</td>
                    <td className="px-4 h-9 tabular-nums">{s.matched}</td>
                    <td className="px-4 h-9 tabular-nums text-success">
                      {s.succeeded}
                    </td>
                    <td className="px-4 h-9 tabular-nums text-destructive">
                      {s.failed}
                    </td>
                    <td className="px-4 h-9 tabular-nums">{s.skipped}</td>
                    <td className="px-4 h-9 tabular-nums">
                      {s.success_rate.toFixed(1)}%
                    </td>
                    <td className={`px-4 h-9 whitespace-nowrap ${status.className}`}>
                      {status.label}
                    </td>
                  </tr>
                )
              })}
            </tbody>
          </table>
        </div>
      )}
    </section>
  )
}

function breakerLabel(status: string): { label: string; className: string } {
  if (status === 'tripped') return { label: '已暂停', className: 'text-destructive' }
  if (status === 'ok') return { label: '正常', className: 'text-success' }
  return { label: '—', className: 'text-muted-foreground' }
}

function UserInfoSection({ info }: { info: UserInfoAggregate }) {
  return (
    <section className="rounded-lg border border-border bg-card p-5">
      <h2 className="mb-4 text-base font-semibold text-foreground">
        各站点账号概览
      </h2>

      <div className="grid grid-cols-2 lg:grid-cols-4 gap-4 mb-5">
        <StatCard label="总上传" value={formatBytes(info.total_uploaded)} accent="success" />
        <StatCard label="总下载" value={formatBytes(info.total_downloaded)} accent="accent" />
        <StatCard label="总做种数" value={String(info.total_seeding)} accent="accent" />
        <StatCard label="总积分" value={info.total_bonus.toFixed(1)} accent="accent" />
      </div>

      {info.sites.length === 0 ? (
        <EmptyState title="暂无用户统计数据。" />
      ) : (
        <div className="overflow-x-auto border border-border rounded-lg">
          <table className="w-full border-collapse text-sm font-body">
            <thead>
              <tr className="bg-muted border-b border-border">
                <Th>站点</Th>
                <Th>上传量</Th>
                <Th>下载量</Th>
                <Th>分享率</Th>
                <Th>积分</Th>
                <Th secondary>等级</Th>
                <Th>做种数</Th>
                <Th secondary>吸血数</Th>
                <Th secondary>做种体积</Th>
                <Th secondary>上传时间</Th>
                <Th secondary>更新时间</Th>
              </tr>
            </thead>
            <tbody>
              {info.sites.map((s) => (
                <UserInfoRow key={s.site_id} site={s} />
              ))}
            </tbody>
          </table>
        </div>
      )}
    </section>
  )
}

function Th({ children, secondary }: { children: string; secondary?: boolean }) {
  return (
    <th
      className={[
        'text-left px-4 h-9 text-xs font-medium text-muted-foreground whitespace-nowrap',
        secondary ? 'hidden lg:table-cell' : '',
      ].join(' ')}
    >
      {children}
    </th>
  )
}

function Td({
  children,
  secondary,
  muted,
}: {
  children: string
  secondary?: boolean
  muted?: boolean
}) {
  return (
    <td
      className={[
        'px-4 h-9 whitespace-nowrap tabular-nums',
        secondary ? 'hidden lg:table-cell' : '',
        muted ? 'text-muted-foreground' : '',
      ].join(' ')}
    >
      {children}
    </td>
  )
}

function UserInfoRow({ site }: { site: SiteUserInfo }) {
  return (
    <tr className="border-b border-border last:border-b-0 hover:bg-muted/50 transition-colors duration-150">
      <Td>{site.site_name}</Td>
      <Td>{site.uploaded != null ? formatBytes(site.uploaded) : '-'}</Td>
      <Td>{site.downloaded != null ? formatBytes(site.downloaded) : '-'}</Td>
      <Td>{site.ratio != null ? site.ratio.toFixed(3) : '-'}</Td>
      <Td>{site.bonus != null ? site.bonus.toFixed(1) : '-'}</Td>
      <Td secondary>{site.user_class ?? '-'}</Td>
      <Td>{site.seeding_count != null ? String(site.seeding_count) : '-'}</Td>
      <Td secondary>{site.leeching_count != null ? String(site.leeching_count) : '-'}</Td>
      <Td secondary>{site.seeding_size != null ? formatBytes(site.seeding_size) : '-'}</Td>
      <Td secondary>
        {site.upload_time_seconds != null ? formatDuration(site.upload_time_seconds) : '-'}
      </Td>
      <Td secondary muted>
        {site.fetched_at ? formatShortTime(site.fetched_at) : '-'}
      </Td>
    </tr>
  )
}
