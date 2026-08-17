import { useState, type FormEvent } from 'react'
import { useParams, useNavigate } from 'react-router'
import { toast } from 'sonner'
import {
  useProbeSite,
  useRefreshSiteStats,
  useSiteDetail,
  useUpdateSite,
} from '../../api/hooks/sites'
import type { UpdateSiteInput } from '../../api/types'
import { LoadingSkeleton, PageHeader } from '../../components/shared'
import { Button, Input } from '../../components/ui'
import { formatBytes, formatDuration } from '../../lib/time'

interface ProbeFieldResult {
  field_name: string
  success: boolean
  value_preview?: string
  error?: string
}

interface ProbeDetail {
  api_reachable?: ProbeFieldResult
  user_info_fields: ProbeFieldResult[]
  passkey_error?: string
}

interface EditFormState {
  url: string
  api_url: string
  cookie: string
  passkey: string
  rate_limit_interval_ms: string
  rate_limit_burst: string
  download_interval_ms: string
}

const FIELD_LABELS: Record<string, string> = {
  api_reachable: '辅种 API',
  uploaded: '上传量',
  downloaded: '下载量',
  ratio: '分享率',
  bonus: '积分/魔力值',
  user_class: '用户等级',
  seeding_count: '做种数',
  leeching_count: '下载中数量',
  seeding_size: '做种体积',
  upload_time_seconds: '做种时间',
}

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

function parseProbeDetail(json?: string | null): ProbeDetail | null {
  if (!json) return null
  try {
    return JSON.parse(json) as ProbeDetail
  } catch {
    return null
  }
}

function fieldLabel(name: string): string {
  return FIELD_LABELS[name] ?? name
}

export default function SiteDetailPage() {
  const params = useParams<{ id: string }>()
  const navigate = useNavigate()
  const siteId = Number(params.id) || 0

  const detail = useSiteDetail(siteId)
  const updateSite = useUpdateSite(siteId)
  const probeSite = useProbeSite()
  const refreshStats = useRefreshSiteStats()

  const [editing, setEditing] = useState(false)
  const [editForm, setEditForm] = useState<EditFormState>({
    url: '',
    api_url: '',
    cookie: '',
    passkey: '',
    rate_limit_interval_ms: '',
    rate_limit_burst: '',
    download_interval_ms: '',
  })

  const site = detail.data?.site
  const userStats = detail.data?.user_stats
  const probeDetailJson = detail.data?.probe_detail ?? site?.probe_detail_json
  const probeDetail = parseProbeDetail(probeDetailJson)

  const startEdit = () => {
    if (!site) return
    setEditForm({
      url: site.url,
      api_url: site.api_url ?? '',
      cookie: '',
      passkey: '',
      rate_limit_interval_ms: site.rate_limit_interval_ms != null ? String(site.rate_limit_interval_ms) : '',
      rate_limit_burst: site.rate_limit_burst != null ? String(site.rate_limit_burst) : '',
      download_interval_ms: site.download_interval_ms != null ? String(site.download_interval_ms) : '',
    })
    setEditing(true)
  }

  const handleEditSubmit = (event: FormEvent) => {
    event.preventDefault()
    const input: UpdateSiteInput = {
      url: editForm.url.trim(),
      api_url: editForm.api_url.trim() || undefined,
      cookie: editForm.cookie || undefined,
      passkey: editForm.passkey || undefined,
      rate_limit_interval_ms: editForm.rate_limit_interval_ms ? Number(editForm.rate_limit_interval_ms) : undefined,
      rate_limit_burst: editForm.rate_limit_burst ? Number(editForm.rate_limit_burst) : undefined,
      download_interval_ms: editForm.download_interval_ms ? Number(editForm.download_interval_ms) : undefined,
    }
    updateSite.mutate(input, {
      onSuccess: () => {
        toast.success('站点更新成功')
        setEditing(false)
      },
      onError: (err) => {
        toast.error(`更新站点失败：${formatApiError(err, '未知错误')}`)
      },
    })
  }

  const handleProbe = () => {
    probeSite.mutate(siteId, {
      onSuccess: (result) => {
        if (result.status === 'success') {
          toast.success('探测成功')
        } else {
          toast.error(`探测失败：${result.message}`)
        }
      },
      onError: (err) => {
        toast.error(`探测失败：${formatApiError(err, '未知错误')}`)
      },
    })
  }

  const handleRefreshStats = () => {
    refreshStats.mutate(siteId, {
      onSuccess: () => {
        toast.success('统计数据已刷新')
      },
      onError: (err) => {
        toast.error(`刷新统计失败：${formatApiError(err, '未知错误')}`)
      },
    })
  }

  if (detail.isLoading) {
    return (
      <div>
        <PageHeader title="站点详情" />
        <LoadingSkeleton variant="card" rows={6} />
      </div>
    )
  }

  if (detail.isError || !site) {
    return (
      <div>
        <PageHeader title="站点详情" />
        <div className="flex flex-col items-center justify-center py-6 gap-3">
          <p className="text-sm text-destructive m-0">
            加载站点详情失败：{detail.error ? formatApiError(detail.error, '未知错误') : '站点不存在'}
          </p>
          <Button variant="secondary" size="sm" onClick={() => navigate('/sites')}>
            返回列表
          </Button>
        </div>
      </div>
    )
  }

  return (
    <div>
      <PageHeader
        title={site.name}
        actions={
          <div className="flex items-center gap-1">
            <Button variant="secondary" size="sm" onClick={() => navigate('/sites')}>
              返回列表
            </Button>
            <Button
              variant="secondary"
              size="sm"
              loading={refreshStats.isPending}
              onClick={handleRefreshStats}
            >
              刷新统计
            </Button>
            <Button
              variant="secondary"
              size="sm"
              loading={probeSite.isPending}
              onClick={handleProbe}
            >
              重新探测
            </Button>
            <Button
              variant={editing ? 'secondary' : 'primary'}
              size="sm"
              onClick={() => {
                if (editing) {
                  setEditing(false)
                } else {
                  startEdit()
                }
              }}
            >
              {editing ? '取消编辑' : '编辑'}
            </Button>
          </div>
        }
      />

      {/* Site info card */}
      <div className="mb-5 p-5 border border-border rounded-lg bg-card">
        <h3 className="text-sm font-medium text-foreground mt-0 mb-3">
          站点信息
        </h3>
        <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-3">
          <InfoItem label="URL" value={site.url} />
          <InfoItem label="API URL" value={site.api_url ?? '—'} />
          <InfoItem label="适配器" value={site.adapter_type} />
          <InfoItem label="凭据类型" value={site.auth_type} />
          <InfoItem label="速率限制间隔" value={site.rate_limit_interval_ms != null ? `${site.rate_limit_interval_ms} ms` : '—'} />
          <InfoItem label="速率限制突发" value={site.rate_limit_burst != null ? String(site.rate_limit_burst) : '—'} />
          <InfoItem label="下载间隔" value={site.download_interval_ms != null ? `${site.download_interval_ms} ms` : '—'} />
          <InfoItem
            label="探测状态"
            value={site.probe_status === 'success' || site.probe_status === '成功' ? '成功' : site.probe_status === 'failed' || site.probe_status === '失败' ? '失败' : '未探测'}
            valueClassName={
              site.probe_status === 'success' || site.probe_status === '成功'
                ? 'text-success'
                : site.probe_status === 'failed' || site.probe_status === '失败'
                  ? 'text-destructive'
                  : 'text-muted-foreground'
            }
          />
          <InfoItem
            label="启用"
            value={site.enabled ? '是' : '否'}
            valueClassName={site.enabled ? 'text-success' : 'text-destructive'}
          />
        </div>
      </div>

      {/* Edit form */}
      {editing && (
        <form
          onSubmit={handleEditSubmit}
          className="mb-5 p-5 border border-border rounded-lg bg-card"
        >
          <h3 className="text-sm font-medium text-foreground mt-0 mb-3">
            编辑站点
          </h3>
          <div className="grid grid-cols-1 sm:grid-cols-2 gap-5">
            <Input
              label="URL"
              value={editForm.url}
              onChange={(e) => setEditForm((prev) => ({ ...prev, url: e.target.value }))}
              autoComplete="off"
            />
            <Input
              label="API URL"
              value={editForm.api_url}
              onChange={(e) => setEditForm((prev) => ({ ...prev, api_url: e.target.value }))}
              autoComplete="off"
            />
            <Input
              label="Cookie（留空不修改）"
              value={editForm.cookie}
              onChange={(e) => setEditForm((prev) => ({ ...prev, cookie: e.target.value }))}
              autoComplete="off"
            />
            <Input
              label="Passkey（留空不修改）"
              value={editForm.passkey}
              onChange={(e) => setEditForm((prev) => ({ ...prev, passkey: e.target.value }))}
              autoComplete="off"
            />
            <Input
              label="速率限制间隔 (ms)"
              type="number"
              value={editForm.rate_limit_interval_ms}
              onChange={(e) => setEditForm((prev) => ({ ...prev, rate_limit_interval_ms: e.target.value }))}
            />
            <Input
              label="速率限制突发"
              type="number"
              value={editForm.rate_limit_burst}
              onChange={(e) => setEditForm((prev) => ({ ...prev, rate_limit_burst: e.target.value }))}
            />
            <Input
              label="下载间隔 (ms)"
              type="number"
              value={editForm.download_interval_ms}
              onChange={(e) => setEditForm((prev) => ({ ...prev, download_interval_ms: e.target.value }))}
            />
          </div>
          <div className="flex justify-end mt-4">
            <Button type="submit" loading={updateSite.isPending}>
              {updateSite.isPending ? '保存中...' : '保存'}
            </Button>
          </div>
        </form>
      )}

      {/* Probe detail */}
      {probeDetail && (
        <div className="mb-5 p-5 border border-border rounded-lg bg-card">
          <h3 className="text-sm font-medium text-foreground mt-0 mb-3">
            探测详情
          </h3>

          {probeDetail.passkey_error && (
            <p className="text-sm text-destructive mb-2">
              Passkey 错误：{probeDetail.passkey_error}
            </p>
          )}

          {probeDetail.api_reachable && (
            <div className="mb-2">
              <ProbeFieldRow field={probeDetail.api_reachable} />
            </div>
          )}

          {probeDetail.user_info_fields.length > 0 && (
            <div className="overflow-x-auto border border-border rounded-md">
              <table className="w-full border-collapse text-sm">
                <thead>
                  <tr className="bg-muted border-b border-border">
                    {['字段', '状态', '预览值', '错误'].map((h) => (
                      <th
                        key={h}
                        className="text-left px-4 h-7 text-xs font-medium text-foreground/70 whitespace-nowrap"
                      >
                        {h}
                      </th>
                    ))}
                  </tr>
                </thead>
                <tbody>
                  {probeDetail.user_info_fields.map((f) => (
                    <tr
                      key={f.field_name}
                      className="border-b border-border last:border-b-0"
                    >
                      <td className="px-4 h-7 text-foreground whitespace-nowrap">
                        {fieldLabel(f.field_name)}
                      </td>
                      <td
                        className={`px-4 h-7 whitespace-nowrap ${
                          f.success ? 'text-success' : 'text-destructive'
                        }`}
                      >
                        {f.success ? '成功' : '失败'}
                      </td>
                      <td className="px-4 h-7 text-muted-foreground whitespace-nowrap">
                        {f.value_preview ?? '—'}
                      </td>
                      <td className="px-4 h-7 text-destructive whitespace-nowrap">
                        {f.error ?? '—'}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </div>
      )}

      {/* User stats */}
      {userStats && (
        <div className="mb-5 p-5 border border-border rounded-lg bg-card">
          <h3 className="text-sm font-medium text-foreground mt-0 mb-3">
            用户统计
          </h3>
          <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-3">
            {userStats.uploaded != null && (
              <InfoItem label="上传量" value={formatBytes(userStats.uploaded)} />
            )}
            {userStats.downloaded != null && (
              <InfoItem label="下载量" value={formatBytes(userStats.downloaded)} />
            )}
            {userStats.ratio != null && (
              <InfoItem label="分享率" value={userStats.ratio.toFixed(3)} />
            )}
            {userStats.bonus != null && (
              <InfoItem label="积分/魔力值" value={userStats.bonus.toLocaleString()} />
            )}
            {userStats.user_class && (
              <InfoItem label="用户等级" value={userStats.user_class} />
            )}
            {userStats.seeding_count != null && (
              <InfoItem label="做种数" value={String(userStats.seeding_count)} />
            )}
            {userStats.leeching_count != null && (
              <InfoItem label="下载中数量" value={String(userStats.leeching_count)} />
            )}
            {userStats.seeding_size != null && (
              <InfoItem label="做种体积" value={formatBytes(userStats.seeding_size)} />
            )}
            {userStats.upload_time_seconds != null && (
              <InfoItem label="做种时间" value={formatDuration(userStats.upload_time_seconds)} />
            )}
          </div>
        </div>
      )}
    </div>
  )
}

function InfoItem({
  label,
  value,
  valueClassName,
}: {
  label: string
  value: string
  valueClassName?: string
}) {
  return (
    <div className="flex flex-col gap-0.5">
      <span className="text-xs text-foreground/70 font-medium">
        {label}
      </span>
      <span className={`text-sm ${valueClassName ?? 'text-foreground'}`}>
        {value}
      </span>
    </div>
  )
}

function ProbeFieldRow({ field }: { field: ProbeFieldResult }) {
  return (
    <div className="flex items-center gap-2 text-sm">
      <span className="text-foreground/70 font-medium">
        {fieldLabel(field.field_name)}:
      </span>
      <span className={field.success ? 'text-success' : 'text-destructive'}>
        {field.success ? '成功' : '失败'}
      </span>
      {field.value_preview && (
        <span className="text-muted-foreground">({field.value_preview})</span>
      )}
      {field.error && (
        <span className="text-destructive">{field.error}</span>
      )}
    </div>
  )
}
