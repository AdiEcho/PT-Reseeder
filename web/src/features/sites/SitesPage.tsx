import { useState, type FormEvent } from 'react'
import { useNavigate } from 'react-router'
import { toast } from 'sonner'
import {
  useCreateSite,
  useDeleteSite,
  useProbeSite,
  useSiteDefinitions,
  useSites,
  useValidateSite,
} from '../../api/hooks/sites'
import type { SiteDefinitionInfo, SiteInfo } from '../../api/types'
import { ConfirmDialog, EmptyState, LoadingSkeleton, PageHeader, StatusBadge } from '../../components/shared'
import { DataTable, type Column } from '../../components/shared/DataTable'
import { Badge, Button, Input, Select } from '../../components/ui'

type AuthType = 'cookie' | 'passkey'

interface FormState {
  definition_id: string
  name: string
  url: string
  api_url: string
  adapter_type: string
  auth_type: AuthType
  cookie: string
  passkey: string
  rate_limit_interval_ms: string
  rate_limit_burst: string
  download_interval_ms: string
}

interface FieldErrors {
  name?: string
  url?: string
}

const INITIAL_FORM: FormState = {
  definition_id: '',
  name: '',
  url: '',
  api_url: '',
  adapter_type: '',
  auth_type: 'cookie',
  cookie: '',
  passkey: '',
  rate_limit_interval_ms: '',
  rate_limit_burst: '',
  download_interval_ms: '',
}

const AUTH_TYPE_OPTIONS = [
  { value: 'cookie', label: 'Cookie' },
  { value: 'passkey', label: 'Passkey' },
]

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

function validateFormFields(form: FormState): FieldErrors {
  const errors: FieldErrors = {}
  if (!form.name.trim()) {
    errors.name = '名称不能为空'
  }
  if (!form.url.trim()) {
    errors.url = 'URL 不能为空'
  }
  return errors
}

export default function SitesPage() {
  const navigate = useNavigate()
  const sites = useSites()
  const definitions = useSiteDefinitions()
  const createSite = useCreateSite()
  const deleteSite = useDeleteSite()
  const validateSite = useValidateSite(0)
  const probeSite = useProbeSite()

  const [showForm, setShowForm] = useState(false)
  const [form, setForm] = useState<FormState>(INITIAL_FORM)
  const [fieldErrors, setFieldErrors] = useState<FieldErrors>({})
  const [deleteTarget, setDeleteTarget] = useState<SiteInfo | null>(null)
  const [validateResult, setValidateResult] = useState<{ ok: boolean; message: string } | null>(null)

  const resetForm = () => {
    setForm(INITIAL_FORM)
    setFieldErrors({})
    setValidateResult(null)
  }

  const updateField = <K extends keyof FormState>(key: K, value: FormState[K]) => {
    setForm((prev) => ({ ...prev, [key]: value }))
    if (key === 'name' || key === 'url') {
      setFieldErrors((prev) => ({ ...prev, [key]: undefined }))
    }
  }

  const handleDefinitionChange = (value: string) => {
    const defs = definitions.data ?? []
    const def = defs.find((d: SiteDefinitionInfo) => d.id === value)
    if (def) {
      setForm((prev) => ({
        ...prev,
        definition_id: value,
        name: def.name,
        url: def.url,
        api_url: def.api_url ?? '',
        adapter_type: def.adapter,
        rate_limit_interval_ms: def.rate_limit_interval_ms != null ? String(def.rate_limit_interval_ms) : '',
        rate_limit_burst: def.rate_limit_burst != null ? String(def.rate_limit_burst) : '',
        download_interval_ms: def.download_interval_ms != null ? String(def.download_interval_ms) : '',
      }))
    } else {
      setForm((prev) => ({ ...prev, definition_id: value }))
    }
    setFieldErrors({})
    setValidateResult(null)
  }

  const handleSubmit = (event: FormEvent) => {
    event.preventDefault()
    const errors = validateFormFields(form)
    setFieldErrors(errors)
    if (errors.name || errors.url) return

    createSite.mutate(
      {
        name: form.name.trim(),
        url: form.url.trim(),
        api_url: form.api_url.trim() || undefined,
        adapter_type: form.adapter_type,
        auth_type: form.auth_type,
        cookie: form.auth_type === 'cookie' ? form.cookie || undefined : undefined,
        passkey: form.auth_type === 'passkey' ? form.passkey || undefined : undefined,
        rate_limit_interval_ms: form.rate_limit_interval_ms ? Number(form.rate_limit_interval_ms) : undefined,
        rate_limit_burst: form.rate_limit_burst ? Number(form.rate_limit_burst) : undefined,
        download_interval_ms: form.download_interval_ms ? Number(form.download_interval_ms) : undefined,
      },
      {
        onSuccess: () => {
          toast.success('站点创建成功')
          resetForm()
          setShowForm(false)
        },
        onError: (err) => {
          toast.error(`创建站点失败：${formatApiError(err, '未知错误')}`)
        },
      },
    )
  }

  const handleValidate = () => {
    validateSite.mutate(
      {
        name: form.name.trim(),
        url: form.url.trim(),
        api_url: form.api_url.trim() || undefined,
        adapter_type: form.adapter_type,
        cookie: form.auth_type === 'cookie' ? form.cookie || undefined : undefined,
        passkey: form.auth_type === 'passkey' ? form.passkey || undefined : undefined,
      },
      {
        onSuccess: (result) => {
          setValidateResult({ ok: result.status === 'success', message: result.message })
        },
        onError: (err) => {
          setValidateResult({ ok: false, message: formatApiError(err, '验证失败') })
        },
      },
    )
  }

  const handleProbe = (id: number) => {
    probeSite.mutate(id, {
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

  const handleConfirmDelete = () => {
    if (!deleteTarget) return
    deleteSite.mutate(deleteTarget.id, {
      onSuccess: () => {
        toast.success('站点已删除')
        setDeleteTarget(null)
      },
      onError: (err) => {
        toast.error(`删除站点失败：${formatApiError(err, '未知错误')}`)
      },
    })
  }

  const list = sites.data ?? []
  const defs = definitions.data ?? []
  const defOptions = [
    { value: '', label: '— 选择站点模板 —' },
    ...defs.map((d: SiteDefinitionInfo) => ({ value: d.id, label: d.name })),
  ]
  const probingId = probeSite.isPending ? probeSite.variables : undefined

  const siteColumns: Column<SiteInfo>[] = [
    { key: 'name', header: '名称' },
    {
      key: 'url',
      header: 'URL',
      render: (site) => (
        <span className="max-w-[200px] truncate block">{site.url}</span>
      ),
    },
    { key: 'adapter_type', header: '适配器' },
    { key: 'auth_type', header: '凭据类型' },
    {
      key: 'probe_status',
      header: '探测状态',
      render: (site) => <StatusBadge domain="site" status={site.probe_status || 'unknown'} />,
    },
    {
      key: 'enabled',
      header: '启用',
      render: (site) => (
        <Badge variant={site.enabled ? 'success' : 'destructive'}>
          {site.enabled ? '是' : '否'}
        </Badge>
      ),
    },
    {
      key: 'actions',
      header: '操作',
      render: (site) => (
        <div
          className="flex items-center gap-1"
          onClick={(e) => e.stopPropagation()}
        >
          <Button
            variant="secondary"
            size="sm"
            loading={probingId === site.id}
            onClick={() => handleProbe(site.id)}
          >
            探测
          </Button>
          <Button
            variant="danger"
            size="sm"
            onClick={() => setDeleteTarget(site)}
          >
            删除
          </Button>
        </div>
      ),
    },
  ]

  return (
    <div>
      <PageHeader
        title="站点管理"
        actions={
          <Button
            variant={showForm ? 'secondary' : 'primary'}
            size="sm"
            onClick={() => {
              setShowForm((open) => !open)
              if (showForm) resetForm()
            }}
          >
            {showForm ? '取消' : '添加站点'}
          </Button>
        }
      />

      <p className="text-sm text-muted-foreground mb-4">
        管理 PT 站点的连接信息与凭据，支持探测验证站点可用性。
      </p>

      {showForm && (
        <form
          onSubmit={handleSubmit}
          className="mb-5 p-5 border border-border rounded-lg bg-card"
        >
          <div className="grid grid-cols-1 sm:grid-cols-2 gap-5">
            <Select
              label="站点模板"
              options={defOptions}
              value={form.definition_id}
              onChange={(event) => handleDefinitionChange(event.target.value)}
            />
            <Input
              label="名称 *"
              placeholder="站点名称"
              value={form.name}
              error={fieldErrors.name}
              onChange={(event) => updateField('name', event.target.value)}
              autoComplete="off"
            />
            <Input
              label="URL *"
              placeholder="https://example.com"
              value={form.url}
              error={fieldErrors.url}
              onChange={(event) => updateField('url', event.target.value)}
              autoComplete="off"
            />
            <Input
              label="API URL"
              placeholder="https://example.com/api"
              value={form.api_url}
              onChange={(event) => updateField('api_url', event.target.value)}
              autoComplete="off"
            />
            <Input
              label="适配器类型"
              placeholder="nexusphp"
              value={form.adapter_type}
              onChange={(event) => updateField('adapter_type', event.target.value)}
              autoComplete="off"
            />
            <Select
              label="凭据类型"
              options={AUTH_TYPE_OPTIONS}
              value={form.auth_type}
              onChange={(event) => updateField('auth_type', event.target.value as AuthType)}
            />
            {form.auth_type === 'cookie' && (
              <Input
                label="Cookie"
                placeholder="Cookie 值"
                value={form.cookie}
                onChange={(event) => updateField('cookie', event.target.value)}
                autoComplete="off"
              />
            )}
            {form.auth_type === 'passkey' && (
              <Input
                label="Passkey"
                placeholder="Passkey 值"
                value={form.passkey}
                onChange={(event) => updateField('passkey', event.target.value)}
                autoComplete="off"
              />
            )}
            <Input
              label="速率限制间隔 (ms)"
              type="number"
              placeholder="5000"
              value={form.rate_limit_interval_ms}
              onChange={(event) => updateField('rate_limit_interval_ms', event.target.value)}
            />
            <Input
              label="速率限制突发"
              type="number"
              placeholder="5"
              value={form.rate_limit_burst}
              onChange={(event) => updateField('rate_limit_burst', event.target.value)}
            />
            <Input
              label="下载间隔 (ms)"
              type="number"
              placeholder="2000"
              value={form.download_interval_ms}
              onChange={(event) => updateField('download_interval_ms', event.target.value)}
            />
          </div>

          {validateResult && (
            <p
              className={`mt-2 mb-0 text-sm ${
                validateResult.ok ? 'text-success' : 'text-destructive'
              }`}
            >
              {validateResult.message}
            </p>
          )}

          <div className="flex justify-end gap-2 mt-4">
            <Button
              variant="secondary"
              type="button"
              loading={validateSite.isPending}
              onClick={handleValidate}
            >
              {validateSite.isPending ? '验证中...' : '验证'}
            </Button>
            <Button type="submit" loading={createSite.isPending}>
              {createSite.isPending ? '创建中...' : '创建'}
            </Button>
          </div>
        </form>
      )}

      {sites.isLoading && <LoadingSkeleton variant="table" rows={5} columns={5} />}

      {sites.isError && (
        <div className="flex flex-col items-center justify-center py-6 gap-3">
          <p className="text-sm text-destructive m-0">
            加载站点失败：{formatApiError(sites.error, '未知错误')}
          </p>
          <Button variant="secondary" size="sm" onClick={() => sites.refetch()}>
            重试
          </Button>
        </div>
      )}

      {!sites.isLoading && !sites.isError && list.length === 0 && (
        <EmptyState
          title="尚未配置任何站点。"
          description="添加 PT 站点后即可用于辅种任务。"
          actionLabel={showForm ? undefined : '添加站点'}
          onAction={showForm ? undefined : () => setShowForm(true)}
        />
      )}

      {!sites.isLoading && !sites.isError && list.length > 0 && (
        <DataTable
          columns={siteColumns}
          data={list as unknown as Record<string, unknown>[]}
          caption="站点列表"
          onRowClick={(row) => navigate(`/sites/${(row as unknown as SiteInfo).id}`)}
        />
      )}

      <ConfirmDialog
        open={deleteTarget != null}
        title="确认删除"
        message={
          deleteTarget
            ? `确定要删除站点「${deleteTarget.name}」吗？此操作不可撤销。`
            : ''
        }
        confirmLabel="确认删除"
        cancelLabel="取消"
        danger
        loading={deleteSite.isPending}
        onConfirm={handleConfirmDelete}
        onCancel={() => {
          if (!deleteSite.isPending) setDeleteTarget(null)
        }}
      />
    </div>
  )
}
