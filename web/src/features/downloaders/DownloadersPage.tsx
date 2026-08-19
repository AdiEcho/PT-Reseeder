import { useState, type FormEvent } from 'react'
import { toast } from 'sonner'
import {
  useCreateDownloader,
  useDeleteDownloader,
  useDownloaders,
  useTestDownloader,
  useToggleAutoStart,
} from '../../api/hooks/downloaders'
import type { DownloaderInfo } from '../../api/types'
import { ConfirmDialog, EmptyState, LoadingSkeleton, PageHeader } from '../../components/shared'
import { DataTable, type Column } from '../../components/shared/DataTable'
import { Badge, Button, Checkbox, Input, Select, Switch } from '../../components/ui'

type DownloaderType = 'qbittorrent' | 'transmission'
type DownloaderRole = 'source' | 'destination' | 'both'

interface FormState {
  name: string
  dl_type: DownloaderType
  host: string
  port: string
  username: string
  password: string
  role: DownloaderRole
  auto_start: boolean
}

interface FieldErrors {
  name?: string
  host?: string
  port?: string
}

interface TestResult {
  ok: boolean
  message: string
}

const INITIAL_FORM: FormState = {
  name: '',
  dl_type: 'qbittorrent',
  host: '',
  port: '8080',
  username: '',
  password: '',
  role: 'both',
  auto_start: false,
}

const TYPE_OPTIONS = [
  { value: 'qbittorrent', label: 'qBittorrent' },
  { value: 'transmission', label: 'Transmission' },
]

const ROLE_OPTIONS = [
  { value: 'source', label: '仅拉取' },
  { value: 'destination', label: '仅推送' },
  { value: 'both', label: '拉取和推送' },
]

function defaultPortForType(dlType: string): string {
  switch (dlType) {
    case 'qbittorrent':
      return '8080'
    case 'transmission':
      return '9091'
    default:
      return ''
  }
}

function roleLabel(role: string): string {
  switch (role) {
    case 'source':
      return '仅拉取'
    case 'destination':
      return '仅推送'
    case 'both':
      return '拉取和推送'
    default:
      return role
  }
}

function typeLabel(dlType: string): string {
  switch (dlType) {
    case 'qbittorrent':
      return 'qBittorrent'
    case 'transmission':
      return 'Transmission'
    default:
      return dlType
  }
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

function validateFormFields(form: FormState): FieldErrors {
  const errors: FieldErrors = {}
  if (!form.name.trim()) {
    errors.name = '名称不能为空'
  }
  if (!form.host.trim()) {
    errors.host = '主机地址不能为空'
  }
  const trimmedPort = form.port.trim()
  if (!/^\d+$/.test(trimmedPort)) {
    errors.port = '端口必须为数字'
  } else {
    const port = Number.parseInt(trimmedPort, 10)
    if (port < 1 || port > 65535) {
      errors.port = '端口必须在 1–65535 范围内'
    }
  }
  return errors
}

export default function DownloadersPage() {
  const downloaders = useDownloaders()
  const createDownloader = useCreateDownloader()
  const deleteDownloader = useDeleteDownloader()
  const testDownloader = useTestDownloader()
  const toggleAutoStart = useToggleAutoStart()

  const [showForm, setShowForm] = useState(false)
  const [form, setForm] = useState<FormState>(INITIAL_FORM)
  const [fieldErrors, setFieldErrors] = useState<FieldErrors>({})
  const [deleteTarget, setDeleteTarget] = useState<DownloaderInfo | null>(null)
  const [testResults, setTestResults] = useState<Record<number, TestResult>>({})

  const resetForm = () => {
    setForm(INITIAL_FORM)
    setFieldErrors({})
  }

  const updateField = <K extends keyof FormState>(key: K, value: FormState[K]) => {
    setForm((prev) => ({ ...prev, [key]: value }))
    if (key === 'name' || key === 'host' || key === 'port') {
      setFieldErrors((prev) => ({ ...prev, [key]: undefined }))
    }
  }

  const handleTypeChange = (value: string) => {
    const dlType = value as DownloaderType
    setForm((prev) => ({
      ...prev,
      dl_type: dlType,
      port: defaultPortForType(dlType),
    }))
    setFieldErrors({})
  }

  const handleSubmit = (event: FormEvent) => {
    event.preventDefault()
    const errors = validateFormFields(form)
    setFieldErrors(errors)
    if (errors.name || errors.host || errors.port) return

    createDownloader.mutate(
      {
        name: form.name.trim(),
        dl_type: form.dl_type,
        host: form.host.trim(),
        port: Number.parseInt(form.port, 10),
        username: form.username,
        password: form.password,
        role: form.role,
        auto_start: form.auto_start,
      },
      {
        onSuccess: () => {
          toast.success('下载器创建成功')
          resetForm()
          setShowForm(false)
        },
        onError: (err) => {
          toast.error(`创建下载器失败：${formatApiError(err, '未知错误')}`)
        },
      },
    )
  }

  const handleToggleAutoStart = (dl: DownloaderInfo, next: boolean) => {
    toggleAutoStart.mutate(
      { id: dl.id, auto_start: next },
      {
        onSuccess: () => {
          toast.success('自动开始设置已更新')
        },
        onError: (err) => {
          toast.error(`更新自动开始失败：${formatApiError(err, '未知错误')}`)
        },
      },
    )
  }

  const handleTest = (id: number) => {
    testDownloader.mutate(id, {
      onSuccess: (message) => {
        setTestResults((prev) => ({
          ...prev,
          [id]: { ok: true, message },
        }))
      },
      onError: (err) => {
        setTestResults((prev) => ({
          ...prev,
          [id]: {
            ok: false,
            message: `测试失败：${formatApiError(err, '未知错误')}`,
          },
        }))
      },
    })
  }

  const handleConfirmDelete = () => {
    if (!deleteTarget) return
    deleteDownloader.mutate(deleteTarget.id, {
      onSuccess: () => {
        toast.success('下载器已删除')
        setDeleteTarget(null)
        setTestResults((prev) => {
          const next = { ...prev }
          delete next[deleteTarget.id]
          return next
        })
      },
      onError: (err) => {
        toast.error(`删除下载器失败：${formatApiError(err, '未知错误')}`)
      },
    })
  }

  const list = downloaders.data ?? []
  const testingId = testDownloader.isPending ? testDownloader.variables : undefined
  const togglingId = toggleAutoStart.isPending ? toggleAutoStart.variables?.id : undefined

  const dlColumns: Column<DownloaderInfo>[] = [
    { key: 'name', header: '名称' },
    {
      key: 'dl_type',
      header: '类型',
      render: (dl) => <>{typeLabel(dl.dl_type)}</>,
    },
    {
      key: 'host',
      header: '主机:端口',
      render: (dl) => <>{dl.host}:{dl.port}</>,
    },
    {
      key: 'role',
      header: '用途',
      render: (dl) => <>{roleLabel(dl.role)}</>,
    },
    {
      key: 'auto_start',
      header: '自动开始',
      render: (dl) => {
        const autoStartOn = togglingId === dl.id
          ? Boolean(toggleAutoStart.variables?.auto_start)
          : dl.auto_start
        return (
          <div className="inline-flex items-center gap-1.5">
            <Switch
              checked={autoStartOn}
              disabled={togglingId === dl.id}
              onCheckedChange={(checked) => handleToggleAutoStart(dl, checked)}
            />
            <span className={autoStartOn ? 'text-success text-xs' : 'text-muted-foreground text-xs'}>
              {autoStartOn ? '开' : '关'}
            </span>
          </div>
        )
      },
    },
    {
      key: 'enabled',
      header: '启用',
      render: (dl) => (
        <Badge variant={dl.enabled ? 'success' : 'destructive'}>
          {dl.enabled ? '是' : '否'}
        </Badge>
      ),
    },
    {
      key: 'actions',
      header: '操作',
      render: (dl) => {
        const result = testResults[dl.id]
        return (
          <div>
            <div className="flex items-center gap-1">
              <Button
                variant="secondary"
                size="sm"
                loading={testingId === dl.id}
                onClick={() => handleTest(dl.id)}
              >
                测试连接
              </Button>
              <Button
                variant="danger"
                size="sm"
                onClick={() => setDeleteTarget(dl)}
              >
                删除
              </Button>
            </div>
            {result && (
              <p
                className={`mt-0.5 mb-0 text-xs ${result.ok ? 'text-success' : 'text-destructive'}`}
              >
                {result.message}
              </p>
            )}
          </div>
        )
      },
    },
  ]

  return (
    <div>
      <PageHeader
        title="下载器管理"
        actions={
          <Button
            variant={showForm ? 'secondary' : 'primary'}
            size="sm"
            onClick={() => {
              setShowForm((open) => !open)
              if (showForm) resetForm()
            }}
          >
            {showForm ? '取消' : '添加下载器'}
          </Button>
        }
      />

      <p className="text-sm text-muted-foreground mb-4">
        管理 qBittorrent / Transmission 等下载器实例的连接信息。
      </p>

      {showForm && (
        <form
          onSubmit={handleSubmit}
          className="mb-5 p-5 border border-border rounded-lg bg-card"
        >
          <div className="grid grid-cols-1 sm:grid-cols-2 gap-5">
            <Input
              label="名称 *"
              placeholder="我的 qBittorrent"
              value={form.name}
              error={fieldErrors.name}
              onChange={(event) => updateField('name', event.target.value)}
              autoComplete="off"
            />
            <Select
              label="类型"
              options={TYPE_OPTIONS}
              value={form.dl_type}
              onChange={(event) => handleTypeChange(event.target.value)}
            />
            <Input
              label="主机 *"
              placeholder="127.0.0.1"
              value={form.host}
              error={fieldErrors.host}
              onChange={(event) => updateField('host', event.target.value)}
              autoComplete="off"
            />
            <Input
              label="端口 *"
              type="number"
              placeholder="8080"
              min={1}
              max={65535}
              value={form.port}
              error={fieldErrors.port}
              onChange={(event) => updateField('port', event.target.value)}
            />
            <Input
              label="用户名"
              placeholder="admin"
              value={form.username}
              onChange={(event) => updateField('username', event.target.value)}
              autoComplete="username"
            />
            <Input
              label="密码"
              type="password"
              placeholder="密码"
              value={form.password}
              onChange={(event) => updateField('password', event.target.value)}
              autoComplete="current-password"
            />
            <Select
              label="用途"
              options={ROLE_OPTIONS}
              value={form.role}
              onChange={(event) => updateField('role', event.target.value as DownloaderRole)}
            />
            <div className="flex flex-col justify-end gap-0.5">
              <label className="inline-flex items-center gap-1.5 text-sm text-foreground cursor-pointer">
                <Checkbox
                  checked={form.auto_start}
                  onCheckedChange={(checked) => updateField('auto_start', checked === true)}
                />
                添加后自动开始
              </label>
              <p className="text-xs text-muted-foreground m-0">
                关闭时，辅种写入目标下载器的种子会保持暂停，需手动开始。
              </p>
            </div>
          </div>

          <div className="flex justify-end mt-4">
            <Button type="submit" loading={createDownloader.isPending}>
              {createDownloader.isPending ? '创建中...' : '创建'}
            </Button>
          </div>
        </form>
      )}

      {downloaders.isLoading && <LoadingSkeleton variant="table" rows={5} columns={6} />}

      {downloaders.isError && (
        <div className="flex flex-col items-center justify-center py-6 gap-3">
          <p className="text-sm text-destructive m-0">
            加载下载器失败：{formatApiError(downloaders.error, '未知错误')}
          </p>
          <Button variant="secondary" size="sm" onClick={() => downloaders.refetch()}>
            重试
          </Button>
        </div>
      )}

      {!downloaders.isLoading && !downloaders.isError && list.length === 0 && (
        <EmptyState
          title="尚未配置任何下载器。"
          description="添加 qBittorrent 或 Transmission 实例后即可用于辅种。"
          actionLabel={showForm ? undefined : '添加下载器'}
          onAction={showForm ? undefined : () => setShowForm(true)}
        />
      )}

      {!downloaders.isLoading && !downloaders.isError && list.length > 0 && (
        <DataTable
          columns={dlColumns}
          data={list as unknown as Record<string, unknown>[]}
          caption="下载器列表"
        />
      )}

      <ConfirmDialog
        open={deleteTarget != null}
        title="确认删除"
        message={
          deleteTarget
            ? `确定要删除下载器「${deleteTarget.name}」吗？此操作不可撤销。`
            : ''
        }
        confirmLabel="确认删除"
        cancelLabel="取消"
        danger
        loading={deleteDownloader.isPending}
        onConfirm={handleConfirmDelete}
        onCancel={() => {
          if (!deleteDownloader.isPending) setDeleteTarget(null)
        }}
      />
    </div>
  )
}
