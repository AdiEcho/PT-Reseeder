import { Badge } from '../ui/Badge'
import { cn } from '@/lib/cn'
import {
  CheckCircle2,
  XCircle,
  Clock,
  Loader2,
  AlertTriangle,
  Pause,
  SkipForward,
  Eye,
  Send,
  Ban,
  Activity,
  HelpCircle,
} from 'lucide-react'
import type { ComponentType } from 'react'

export interface StatusBadgeProps {
  domain: 'task' | 'reseed' | 'repost' | 'site' | 'downloader' | 'log'
  status: string
  className?: string
}

interface StatusConfig {
  label: string
  variant: 'default' | 'secondary' | 'destructive' | 'success' | 'warning' | 'info' | 'outline' | 'muted'
  icon?: ComponentType<{ className?: string }>
}

const STATUS_MAP: Record<string, Record<string, StatusConfig>> = {
  task: {
    running: { label: '运行中', variant: 'info', icon: Loader2 },
    paused: { label: '已暂停', variant: 'warning', icon: Pause },
    error: { label: '错误', variant: 'destructive', icon: XCircle },
    idle: { label: '空闲', variant: 'muted', icon: Clock },
  },
  reseed: {
    success: { label: '成功', variant: 'success', icon: CheckCircle2 },
    dry_run: { label: '试运行', variant: 'info', icon: Eye },
    failed: { label: '失败', variant: 'destructive', icon: XCircle },
    running: { label: '运行中', variant: 'default', icon: Loader2 },
    partial: { label: '部分成功', variant: 'warning', icon: AlertTriangle },
    skipped: { label: '已跳过', variant: 'muted', icon: SkipForward },
  },
  repost: {
    pending: { label: '待审核', variant: 'warning', icon: Clock },
    approved: { label: '已批准', variant: 'info', icon: CheckCircle2 },
    submitted: { label: '已提交', variant: 'success', icon: Send },
    failed: { label: '失败', variant: 'destructive', icon: XCircle },
    rejected: { label: '已拒绝', variant: 'muted', icon: Ban },
  },
  site: {
    success: { label: '成功', variant: 'success', icon: CheckCircle2 },
    failed: { label: '失败', variant: 'destructive', icon: XCircle },
    unknown: { label: '未探测', variant: 'muted', icon: HelpCircle },
  },
  downloader: {
    online: { label: '在线', variant: 'success', icon: Activity },
    offline: { label: '离线', variant: 'destructive', icon: XCircle },
    unknown: { label: '未知', variant: 'muted', icon: HelpCircle },
  },
  log: {
    success: { label: '成功', variant: 'success', icon: CheckCircle2 },
    dry_run: { label: '试运行', variant: 'info', icon: Eye },
    failed: { label: '失败', variant: 'destructive', icon: XCircle },
    running: { label: '运行中', variant: 'info', icon: Loader2 },
    partial: { label: '部分成功', variant: 'warning', icon: AlertTriangle },
  },
}

function resolveConfig(domain: string, status: string): StatusConfig {
  const domainMap = STATUS_MAP[domain]
  if (domainMap && domainMap[status]) {
    return domainMap[status]
  }
  // Fallback: show raw status with muted variant
  return { label: status, variant: 'muted' }
}

export function StatusBadge({ domain, status, className }: StatusBadgeProps) {
  const config = resolveConfig(domain, status)
  const Icon = config.icon

  return (
    <Badge variant={config.variant} className={cn('inline-flex items-center gap-1', className)}>
      {Icon && <Icon className="h-3 w-3" />}
      {config.label}
    </Badge>
  )
}
