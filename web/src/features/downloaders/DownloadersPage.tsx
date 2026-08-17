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
import { Button, Input, Select } from '../../components/ui'

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

      <p className="text-[var(--text-sm)] text-[var(--color-text-muted)] mb-[var(--space-5)]">
        管理 qBittorrent / Transmission 等下载器实例的连接信息。
      </p>

      {showForm && (
        <form
          onSubmit={handleSubmit}
          className="mb-[var(--space-6)] p-[var(--space-5)] border border-[var(--color-border)] rounded-[var(--radius-md)] bg-[var(--color-bg-elevated)]"
        >
          <div className="grid grid-cols-1 sm:grid-cols-2 gap-[var(--space-5)]">
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
            <div className="flex flex-col justify-end gap-[var(--space-1)]">
              <label className="inline-flex items-center gap-[var(--space-2)] text-[var(--text-sm)] text-[var(--color-text)] cursor-pointer">
                <input
                  type="checkbox"
                  checked={form.auto_start}
                  onChange={(event) => updateField('auto_start', event.target.checked)}
                />
                添加后自动开始
              </label>
              <p className="text-[var(--text-xs)] text-[var(--color-text-muted)] m-0">
                关闭时，辅种写入目标下载器的种子会保持暂停，需手动开始。
              </p>
            </div>
          </div>

          <div className="flex justify-end mt-[var(--space-5)]">
            <Button type="submit" loading={createDownloader.isPending}>
              {createDownloader.isPending ? '创建中...' : '创建'}
            </Button>
          </div>
        </form>
      )}

      {downloaders.isLoading && <LoadingSkeleton variant="table" rows={5} />}

      {downloaders.isError && (
        <div className="flex flex-col items-center justify-center py-[var(--space-8)] gap-[var(--space-4)]">
          <p className="text-[var(--text-sm)] text-[var(--color-error)] m-0">
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
        <div className="overflow-x-auto border border-[var(--color-border)] rounded-[var(--radius-md)]">
          <table className="w-full border-collapse text-[var(--text-sm)]">
            <thead>
              <tr className="bg-[var(--color-bg-subtle)] border-b border-[var(--color-border)]">
                {['名称', '类型', '主机:端口', '用途', '自动开始', '启用', '操作'].map((header) => (
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
              {list.map((dl) => {
                const result = testResults[dl.id]
                const autoStartOn = togglingId === dl.id
                  ? Boolean(toggleAutoStart.variables?.auto_start)
                  : dl.auto_start
                return (
                  <tr
                    key={dl.id}
                    className="border-b border-[var(--color-border-subtle)] last:border-b-0 hover:bg-[var(--color-bg-subtle)] transition-colors duration-[var(--transition-fast)]"
                  >
                    <td className="px-[var(--space-4)] h-7 text-[var(--color-text)] whitespace-nowrap">
                      {dl.name}
                    </td>
                    <td className="px-[var(--space-4)] h-7 text-[var(--color-text)] whitespace-nowrap">
                      {typeLabel(dl.dl_type)}
                    </td>
                    <td className="px-[var(--space-4)] h-7 text-[var(--color-text)] whitespace-nowrap">
                      {dl.host}:{dl.port}
                    </td>
                    <td className="px-[var(--space-4)] h-7 text-[var(--color-text)] whitespace-nowrap">
                      {roleLabel(dl.role)}
                    </td>
                    <td className="px-[var(--space-4)] h-7 whitespace-nowrap">
                      <label className="inline-flex items-center gap-[var(--space-2)] cursor-pointer">
                        <input
                          type="checkbox"
                          checked={autoStartOn}
                          disabled={togglingId === dl.id}
                          onChange={(event) => handleToggleAutoStart(dl, event.target.checked)}
                        />
                        <span
                          className={
                            autoStartOn
                              ? 'text-[var(--color-success)]'
                              : 'text-[var(--color-text-muted)]'
                          }
                        >
                          {autoStartOn ? '开' : '关'}
                        </span>
                      </label>
                    </td>
                    <td
                      className={`px-[var(--space-4)] h-7 whitespace-nowrap ${
                        dl.enabled
                          ? 'text-[var(--color-success)]'
                          : 'text-[var(--color-error)]'
                      }`}
                    >
                      {dl.enabled ? '是' : '否'}
                    </td>
                    <td className="px-[var(--space-4)] py-[var(--space-2)]">
                      <div className="flex items-center gap-[var(--space-2)]">
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
                          className={`mt-[var(--space-1)] mb-0 text-[var(--text-xs)] ${
                            result.ok
                              ? 'text-[var(--color-success)]'
                              : 'text-[var(--color-error)]'
                          }`}
                        >
                          {result.message}
                        </p>
                      )}
                    </td>
                  </tr>
                )
              })}
            </tbody>
          </table>
        </div>
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
