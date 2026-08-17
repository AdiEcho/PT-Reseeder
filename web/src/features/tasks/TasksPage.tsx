import { useState, type FormEvent } from 'react'
import { useMutation, useQueryClient } from '@tanstack/react-query'
import { toast } from 'sonner'
import {
  useCreateTask,
  useDeleteTask,
  useDryRunPreview,
  useTaskLogs,
  useTasks,
  useTriggerTask,
} from '../../api/hooks/tasks'
import { useDownloaders } from '../../api/hooks/downloaders'
import { useFolders } from '../../api/hooks/folders'
import { useSites } from '../../api/hooks/sites'
import { api } from '../../api/client'
import type { CreateTaskInput, TaskInfo } from '../../api/types'
import { ConfirmDialog, EmptyState, LoadingSkeleton, PageHeader } from '../../components/shared'
import { Button, Input, Select } from '../../components/ui'
import { formatBytes, formatDurationMs, formatShortTime } from '../../lib/time'

// --- Constants ---

const TASK_TYPE_OPTIONS = [{ value: 'reseed', label: '辅种' }]

const TRIGGER_TYPE_OPTIONS = [
  { value: 'cron', label: '定时触发' },
  { value: 'manual', label: '手动触发' },
]

interface FormState {
  name: string
  task_type: string
  trigger_type: string
  cron_expression: string
  site_ids: number[]
  folder_ids: number[]
  source_downloader_ids: number[]
  destination_downloader_id: number | undefined
}

interface FieldErrors {
  name?: string
  cron_expression?: string
}

const INITIAL_FORM: FormState = {
  name: '',
  task_type: 'reseed',
  trigger_type: 'cron',
  cron_expression: '',
  site_ids: [],
  folder_ids: [],
  source_downloader_ids: [],
  destination_downloader_id: undefined,
}

// --- Helpers ---

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
    case 'running':
      return { text: '运行中', className: 'text-[var(--color-info)]' }
    case 'paused':
      return { text: '已暂停', className: 'text-[var(--color-warning)]' }
    case 'error':
      return { text: '错误', className: 'text-[var(--color-error)]' }
    default:
      return { text: '空闲', className: 'text-[var(--color-text-muted)]' }
  }
}

function triggerTypeLabel(type: string): string {
  switch (type) {
    case 'cron':
      return '定时'
    case 'manual':
      return '手动'
    default:
      return type
  }
}

function taskTypeLabel(type: string): string {
  switch (type) {
    case 'reseed':
      return '辅种'
    default:
      return type
  }
}

function logStatusLabel(status: string): { text: string; className: string } {
  switch (status) {
    case 'success':
      return { text: '成功', className: 'text-[var(--color-success)]' }
    case 'dry_run':
      return { text: '试运行', className: 'text-[var(--color-info)]' }
    case 'failed':
      return { text: '失败', className: 'text-[var(--color-error)]' }
    case 'running':
      return { text: '运行中', className: 'text-[var(--color-info)]' }
    case 'partial':
      return { text: '部分成功', className: 'text-[var(--color-warning)]' }
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
      return { text: '已识别', className: 'text-[var(--color-info)]' }
    default:
      return { text: outcome ?? '—', className: 'text-[var(--color-text-muted)]' }
  }
}

function validateFormFields(form: FormState): FieldErrors {
  const errors: FieldErrors = {}
  if (!form.name.trim()) {
    errors.name = '名称不能为空'
  }
  if (form.trigger_type === 'cron' && !form.cron_expression.trim()) {
    errors.cron_expression = 'Cron 表达式不能为空'
  }
  return errors
}

// --- Sub-components ---

function TaskLogsPanel({ taskId }: { taskId: number }) {
  const logs = useTaskLogs(taskId)

  if (logs.isLoading) return <LoadingSkeleton variant="table" rows={3} />
  if (logs.isError) {
    return (
      <p className="text-[var(--text-xs)] text-[var(--color-error)] m-0">
        加载日志失败：{formatApiError(logs.error, '未知错误')}
      </p>
    )
  }

  const list = logs.data ?? []
  if (list.length === 0) {
    return <p className="text-[var(--text-xs)] text-[var(--color-text-muted)] m-0">暂无运行日志。</p>
  }

  return (
    <div className="overflow-x-auto border border-[var(--color-border)] rounded-[var(--radius-md)] mt-[var(--space-2)]">
      <table className="w-full border-collapse text-[var(--text-xs)]">
        <thead>
          <tr className="bg-[var(--color-bg-subtle)] border-b border-[var(--color-border)]">
            {['状态', '匹配', '成功', '失败', '耗时', '时间'].map((h) => (
              <th
                key={h}
                className="text-left px-[var(--space-3)] h-6 text-[var(--text-xs)] font-medium text-[var(--color-text-secondary)] whitespace-nowrap"
              >
                {h}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {list.map((log) => {
            const st = logStatusLabel(log.status)
            return (
              <tr
                key={log.id}
                className="border-b border-[var(--color-border-subtle)] last:border-b-0"
              >
                <td className={`px-[var(--space-3)] h-6 whitespace-nowrap ${st.className}`}>
                  {st.text}
                </td>
                <td className="px-[var(--space-3)] h-6 text-[var(--color-text)] whitespace-nowrap">
                  {log.matched_count}
                </td>
                <td className="px-[var(--space-3)] h-6 text-[var(--color-text)] whitespace-nowrap">
                  {log.succeeded_count}
                </td>
                <td className="px-[var(--space-3)] h-6 text-[var(--color-text)] whitespace-nowrap">
                  {log.failed_count}
                </td>
                <td className="px-[var(--space-3)] h-6 text-[var(--color-text)] whitespace-nowrap">
                  {formatDurationMs(log.duration_ms)}
                </td>
                <td className="px-[var(--space-3)] h-6 text-[var(--color-text)] whitespace-nowrap">
                  {formatShortTime(log.created_at)}
                </td>
              </tr>
            )
          })}
        </tbody>
      </table>
    </div>
  )
}

function DryRunPreviewPanel({ taskId }: { taskId: number }) {
  const preview = useDryRunPreview(taskId)

  if (preview.isLoading) return <LoadingSkeleton variant="table" rows={3} />
  if (preview.isError) {
    return (
      <p className="text-[var(--text-xs)] text-[var(--color-error)] m-0">
        加载预览失败：{formatApiError(preview.error, '未知错误')}
      </p>
    )
  }

  const data = preview.data
  if (!data) {
    return <p className="text-[var(--text-xs)] text-[var(--color-text-muted)] m-0">暂无试运行数据。</p>
  }

  return (
    <div className="mt-[var(--space-2)]">
      <p className="text-[var(--text-xs)] text-[var(--color-text-secondary)] m-0 mb-[var(--space-2)]">
        预计新增：<span className="font-medium text-[var(--color-info)]">{data.would_add_count}</span> 条
      </p>
      {data.items.length > 0 && (
        <div className="overflow-x-auto border border-[var(--color-border)] rounded-[var(--radius-md)]">
          <table className="w-full border-collapse text-[var(--text-xs)]">
            <thead>
              <tr className="bg-[var(--color-bg-subtle)] border-b border-[var(--color-border)]">
                {['站点', '标题', '保存路径', '大小', '结果'].map((h) => (
                  <th
                    key={h}
                    className="text-left px-[var(--space-3)] h-6 text-[var(--text-xs)] font-medium text-[var(--color-text-secondary)] whitespace-nowrap"
                  >
                    {h}
                  </th>
                ))}
              </tr>
            </thead>
            <tbody>
              {data.items.map((item, idx) => {
                const oc = outcomeLabel(item.outcome)
                return (
                  <tr
                    key={`${item.pieces_hash}-${idx}`}
                    className="border-b border-[var(--color-border-subtle)] last:border-b-0"
                  >
                    <td className="px-[var(--space-3)] h-6 text-[var(--color-text)] whitespace-nowrap">
                      {item.site_name}
                    </td>
                    <td className="px-[var(--space-3)] h-6 text-[var(--color-text)] max-w-[200px] truncate">
                      {item.title ?? '—'}
                    </td>
                    <td className="px-[var(--space-3)] h-6 text-[var(--color-text)] max-w-[200px] truncate">
                      {item.save_path}
                    </td>
                    <td className="px-[var(--space-3)] h-6 text-[var(--color-text)] whitespace-nowrap">
                      {item.total_size != null ? formatBytes(item.total_size) : '—'}
                    </td>
                    <td className={`px-[var(--space-3)] h-6 whitespace-nowrap ${oc.className}`}>
                      {oc.text}
                    </td>
                  </tr>
                )
              })}
            </tbody>
          </table>
        </div>
      )}
    </div>
  )
}

// --- TaskForm sub-component ---

interface TaskFormProps {
  form: FormState
  fieldErrors: FieldErrors
  isEditing: boolean
  isPending: boolean
  siteList: { id: number; name: string }[]
  folderList: { id: number; path: string }[]
  sourceDownloaders: { id: number; name: string }[]
  destDownloaders: { id: number; name: string }[]
  onUpdateField: <K extends keyof FormState>(key: K, value: FormState[K]) => void
  onToggleArrayItem: (key: 'site_ids' | 'folder_ids' | 'source_downloader_ids', id: number) => void
  onSubmit: (event: FormEvent) => void
}

function TaskForm({
  form,
  fieldErrors,
  isEditing,
  isPending,
  siteList,
  folderList,
  sourceDownloaders,
  destDownloaders,
  onUpdateField,
  onToggleArrayItem,
  onSubmit,
}: TaskFormProps) {
  return (
    <form
      onSubmit={onSubmit}
      className="mb-[var(--space-6)] p-[var(--space-5)] border border-[var(--color-border)] rounded-[var(--radius-md)] bg-[var(--color-bg-elevated)]"
    >
      <div className="grid grid-cols-1 sm:grid-cols-2 gap-[var(--space-5)]">
        <Input
          label="名称 *"
          placeholder="辅种任务"
          value={form.name}
          error={fieldErrors.name}
          onChange={(event) => onUpdateField('name', event.target.value)}
          autoComplete="off"
        />
        <Select
          label="任务类型"
          options={TASK_TYPE_OPTIONS}
          value={form.task_type}
          onChange={(event) => onUpdateField('task_type', event.target.value)}
        />
        <Select
          label="触发方式"
          options={TRIGGER_TYPE_OPTIONS}
          value={form.trigger_type}
          onChange={(event) => onUpdateField('trigger_type', event.target.value)}
        />
        {form.trigger_type === 'cron' && (
          <Input
            label="Cron 表达式 *"
            placeholder="0 */6 * * *"
            value={form.cron_expression}
            error={fieldErrors.cron_expression}
            onChange={(event) => onUpdateField('cron_expression', event.target.value)}
            autoComplete="off"
          />
        )}
      </div>

      {/* Multi-select: Sites */}
      <div className="mt-[var(--space-5)]">
        <p className="text-[var(--text-sm)] font-medium text-[var(--color-text)] mb-[var(--space-2)] m-0">
          关联站点
        </p>
        {siteList.length === 0 ? (
          <p className="text-[var(--text-xs)] text-[var(--color-text-muted)] m-0">暂无可用站点</p>
        ) : (
          <div className="flex flex-wrap gap-[var(--space-3)]">
            {siteList.map((site) => (
              <label
                key={site.id}
                className="inline-flex items-center gap-[var(--space-1)] text-[var(--text-sm)] text-[var(--color-text)] cursor-pointer"
              >
                <input
                  type="checkbox"
                  checked={form.site_ids.includes(site.id)}
                  onChange={() => onToggleArrayItem('site_ids', site.id)}
                />
                {site.name}
              </label>
            ))}
          </div>
        )}
      </div>

      {/* Multi-select: Folders */}
      <div className="mt-[var(--space-4)]">
        <p className="text-[var(--text-sm)] font-medium text-[var(--color-text)] mb-[var(--space-2)] m-0">
          关联文件夹
        </p>
        {folderList.length === 0 ? (
          <p className="text-[var(--text-xs)] text-[var(--color-text-muted)] m-0">暂无可用文件夹</p>
        ) : (
          <div className="flex flex-wrap gap-[var(--space-3)]">
            {folderList.map((folder) => (
              <label
                key={folder.id}
                className="inline-flex items-center gap-[var(--space-1)] text-[var(--text-sm)] text-[var(--color-text)] cursor-pointer"
              >
                <input
                  type="checkbox"
                  checked={form.folder_ids.includes(folder.id)}
                  onChange={() => onToggleArrayItem('folder_ids', folder.id)}
                />
                {folder.path}
              </label>
            ))}
          </div>
        )}
      </div>

      {/* Multi-select: Source Downloaders */}
      <div className="mt-[var(--space-4)]">
        <p className="text-[var(--text-sm)] font-medium text-[var(--color-text)] mb-[var(--space-2)] m-0">
          源下载器
        </p>
        {sourceDownloaders.length === 0 ? (
          <p className="text-[var(--text-xs)] text-[var(--color-text-muted)] m-0">暂无可用源下载器</p>
        ) : (
          <div className="flex flex-wrap gap-[var(--space-3)]">
            {sourceDownloaders.map((dl) => (
              <label
                key={dl.id}
                className="inline-flex items-center gap-[var(--space-1)] text-[var(--text-sm)] text-[var(--color-text)] cursor-pointer"
              >
                <input
                  type="checkbox"
                  checked={form.source_downloader_ids.includes(dl.id)}
                  onChange={() => onToggleArrayItem('source_downloader_ids', dl.id)}
                />
                {dl.name}
              </label>
            ))}
          </div>
        )}
      </div>

      {/* Single select: Destination Downloader */}
      <div className="mt-[var(--space-4)]">
        <p className="text-[var(--text-sm)] font-medium text-[var(--color-text)] mb-[var(--space-2)] m-0">
          目标下载器
        </p>
        {destDownloaders.length === 0 ? (
          <p className="text-[var(--text-xs)] text-[var(--color-text-muted)] m-0">暂无可用目标下载器</p>
        ) : (
          <Select
            label=""
            options={[
              { value: '', label: '— 不指定 —' },
              ...destDownloaders.map((dl) => ({ value: String(dl.id), label: dl.name })),
            ]}
            value={form.destination_downloader_id != null ? String(form.destination_downloader_id) : ''}
            onChange={(event) => {
              const val = event.target.value
              onUpdateField('destination_downloader_id', val ? Number(val) : undefined)
            }}
          />
        )}
      </div>

      <div className="flex justify-end mt-[var(--space-5)]">
        <Button type="submit" loading={isPending}>
          {isPending ? (isEditing ? '保存中...' : '创建中...') : isEditing ? '保存' : '创建'}
        </Button>
      </div>
    </form>
  )
}

// --- Main Component ---

export default function TasksPage() {
  const queryClient = useQueryClient()
  const tasks = useTasks()
  const createTask = useCreateTask()
  const deleteTask = useDeleteTask()
  const triggerTask = useTriggerTask()
  const sites = useSites()
  const downloaders = useDownloaders()
  const folders = useFolders()

  // Dynamic update mutation (useUpdateTask requires static id; we use inline mutation instead)
  const updateTask = useMutation({
    mutationFn: ({ id, input }: { id: number; input: CreateTaskInput }) =>
      api.put<TaskInfo>(`/api/tasks/${id}`, input),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['tasks'] })
    },
  })

  const [showForm, setShowForm] = useState(false)
  const [editingTask, setEditingTask] = useState<TaskInfo | null>(null)
  const [form, setForm] = useState<FormState>(INITIAL_FORM)
  const [fieldErrors, setFieldErrors] = useState<FieldErrors>({})
  const [deleteTarget, setDeleteTarget] = useState<TaskInfo | null>(null)
  const [expandedLogs, setExpandedLogs] = useState<number | null>(null)
  const [expandedPreview, setExpandedPreview] = useState<number | null>(null)

  const resetForm = () => {
    setForm(INITIAL_FORM)
    setFieldErrors({})
    setEditingTask(null)
  }

  const openCreateForm = () => {
    resetForm()
    setShowForm(true)
  }

  const openEditForm = (task: TaskInfo) => {
    setEditingTask(task)
    setForm({
      name: task.name,
      task_type: task.task_type,
      trigger_type: task.trigger_type,
      cron_expression: task.cron_expression ?? '',
      site_ids: [...task.site_ids],
      folder_ids: [...task.folder_ids],
      source_downloader_ids: [...task.source_downloader_ids],
      destination_downloader_id: task.destination_downloader_id,
    })
    setFieldErrors({})
    setShowForm(true)
  }

  const updateField = <K extends keyof FormState>(key: K, value: FormState[K]) => {
    setForm((prev) => ({ ...prev, [key]: value }))
    if (key === 'name' || key === 'cron_expression') {
      setFieldErrors((prev) => ({ ...prev, [key]: undefined }))
    }
  }

  const toggleArrayItem = (key: 'site_ids' | 'folder_ids' | 'source_downloader_ids', id: number) => {
    setForm((prev) => {
      const arr = prev[key]
      return {
        ...prev,
        [key]: arr.includes(id) ? arr.filter((v) => v !== id) : [...arr, id],
      }
    })
  }

  const handleSubmit = (event: FormEvent) => {
    event.preventDefault()
    const errors = validateFormFields(form)
    setFieldErrors(errors)
    if (errors.name || errors.cron_expression) return

    const input: CreateTaskInput = {
      name: form.name.trim(),
      task_type: form.task_type,
      trigger_type: form.trigger_type,
      cron_expression: form.trigger_type === 'cron' ? form.cron_expression.trim() : undefined,
      site_ids: form.site_ids,
      folder_ids: form.folder_ids,
      source_downloader_ids: form.source_downloader_ids,
      destination_downloader_id: form.destination_downloader_id,
    }

    if (editingTask) {
      updateTask.mutate(
        { id: editingTask.id, input },
        {
          onSuccess: () => {
            toast.success('任务更新成功')
            resetForm()
            setShowForm(false)
          },
          onError: (err) => {
            toast.error(`更新任务失败：${formatApiError(err, '未知错误')}`)
          },
        },
      )
    } else {
      createTask.mutate(input, {
        onSuccess: () => {
          toast.success('任务创建成功')
          resetForm()
          setShowForm(false)
        },
        onError: (err) => {
          toast.error(`创建任务失败：${formatApiError(err, '未知错误')}`)
        },
      })
    }
  }

  const handleTrigger = (id: number, dryRun: boolean) => {
    triggerTask.mutate(
      { id, dryRun },
      {
        onSuccess: () => {
          if (dryRun) {
            toast.success('试运行已触发')
            setExpandedPreview(id)
          } else {
            toast.success('任务已触发')
          }
        },
        onError: (err) => {
          toast.error(`触发任务失败：${formatApiError(err, '未知错误')}`)
        },
      },
    )
  }

  const handleConfirmDelete = () => {
    if (!deleteTarget) return
    deleteTask.mutate(deleteTarget.id, {
      onSuccess: () => {
        toast.success('任务已删除')
        setDeleteTarget(null)
      },
      onError: (err) => {
        toast.error(`删除任务失败：${formatApiError(err, '未知错误')}`)
      },
    })
  }

  const list = tasks.data ?? []
  const siteList = sites.data ?? []
  const downloaderList = downloaders.data ?? []
  const folderList = folders.data ?? []

  const sourceDownloaders = downloaderList.filter(
    (d) => d.role === 'source' || d.role === 'both',
  )
  const destDownloaders = downloaderList.filter(
    (d) => d.role === 'destination' || d.role === 'both',
  )

  const dlMap = new Map(downloaderList.map((d) => [d.id, d.name]))

  function associationSummary(task: TaskInfo): string {
    const parts: string[] = []
    if (task.site_ids.length > 0) parts.push(`站点 ${task.site_ids.length}`)
    if (task.source_downloader_ids.length > 0) parts.push(`源下载器 ${task.source_downloader_ids.length}`)
    if (task.folder_ids.length > 0) parts.push(`文件夹 ${task.folder_ids.length}`)
    if (task.destination_downloader_id != null) {
      const name = dlMap.get(task.destination_downloader_id)
      parts.push(`目标下载器${name ? ` ${name}` : ''}`)
    }
    return parts.length > 0 ? parts.join(' · ') : '—'
  }

  const formIsPending = editingTask ? updateTask.isPending : createTask.isPending

  return (
    <div>
      <PageHeader
        title="任务管理"
        actions={
          <Button
            variant={showForm ? 'secondary' : 'primary'}
            size="sm"
            onClick={() => {
              if (showForm) {
                setShowForm(false)
                resetForm()
              } else {
                openCreateForm()
              }
            }}
          >
            {showForm ? '取消' : '添加任务'}
          </Button>
        }
      />

      <p className="text-[var(--text-sm)] text-[var(--color-text-muted)] mb-[var(--space-5)]">
        管理辅种任务，配置关联站点、下载器和触发方式。
      </p>

      {showForm && (
        <TaskForm
          form={form}
          fieldErrors={fieldErrors}
          isEditing={editingTask != null}
          isPending={formIsPending}
          siteList={siteList}
          folderList={folderList}
          sourceDownloaders={sourceDownloaders}
          destDownloaders={destDownloaders}
          onUpdateField={updateField}
          onToggleArrayItem={toggleArrayItem}
          onSubmit={handleSubmit}
        />
      )}

      {tasks.isLoading && <LoadingSkeleton variant="table" rows={5} />}

      {tasks.isError && (
        <div className="flex flex-col items-center justify-center py-[var(--space-8)] gap-[var(--space-4)]">
          <p className="text-[var(--text-sm)] text-[var(--color-error)] m-0">
            加载任务失败：{formatApiError(tasks.error, '未知错误')}
          </p>
          <Button variant="secondary" size="sm" onClick={() => tasks.refetch()}>
            重试
          </Button>
        </div>
      )}

      {!tasks.isLoading && !tasks.isError && list.length === 0 && (
        <EmptyState
          title="尚未创建任何任务。"
          description="创建辅种任务并关联站点、下载器即可自动化辅种流程。"
          actionLabel={showForm ? undefined : '添加任务'}
          onAction={showForm ? undefined : openCreateForm}
        />
      )}

      {!tasks.isLoading && !tasks.isError && list.length > 0 && (
        <div className="overflow-x-auto border border-[var(--color-border)] rounded-[var(--radius-md)]">
          <table className="w-full border-collapse text-[var(--text-sm)]">
            <thead>
              <tr className="bg-[var(--color-bg-subtle)] border-b border-[var(--color-border)]">
                {['名称', '类型', '触发方式', '状态', '关联', '上次运行', '下次运行', '操作'].map(
                  (header) => (
                    <th
                      key={header}
                      className="text-left px-[var(--space-4)] h-7 text-[var(--text-xs)] font-medium text-[var(--color-text-secondary)] whitespace-nowrap"
                    >
                      {header}
                    </th>
                  ),
                )}
              </tr>
            </thead>
            <tbody>
              {list.map((task) => {
                const st = statusLabel(task.status)
                const isExpLogs = expandedLogs === task.id
                const isExpPreview = expandedPreview === task.id
                return (
                  <tr
                    key={task.id}
                    className="border-b border-[var(--color-border-subtle)] last:border-b-0 hover:bg-[var(--color-bg-subtle)] transition-colors duration-[var(--transition-fast)]"
                  >
                    <td className="px-[var(--space-4)] h-7 text-[var(--color-text)] whitespace-nowrap">
                      {task.name}
                    </td>
                    <td className="px-[var(--space-4)] h-7 text-[var(--color-text)] whitespace-nowrap">
                      {taskTypeLabel(task.task_type)}
                    </td>
                    <td className="px-[var(--space-4)] h-7 text-[var(--color-text)] whitespace-nowrap">
                      {triggerTypeLabel(task.trigger_type)}
                      {task.cron_expression && (
                        <span className="ml-[var(--space-1)] text-[var(--text-xs)] text-[var(--color-text-muted)]">
                          ({task.cron_expression})
                        </span>
                      )}
                    </td>
                    <td className={`px-[var(--space-4)] h-7 whitespace-nowrap ${st.className}`}>
                      {st.text}
                    </td>
                    <td className="px-[var(--space-4)] h-7 text-[var(--color-text-secondary)] whitespace-nowrap text-[var(--text-xs)]">
                      {associationSummary(task)}
                    </td>
                    <td className="px-[var(--space-4)] h-7 text-[var(--color-text)] whitespace-nowrap text-[var(--text-xs)]">
                      {formatShortTime(task.last_run_at)}
                    </td>
                    <td className="px-[var(--space-4)] h-7 text-[var(--color-text)] whitespace-nowrap text-[var(--text-xs)]">
                      {formatShortTime(task.next_run_at)}
                    </td>
                    <td className="px-[var(--space-4)] py-[var(--space-2)]">
                      <div className="flex flex-col gap-[var(--space-2)]">
                        <div className="flex items-center gap-[var(--space-2)] flex-wrap">
                          <Button variant="secondary" size="sm" onClick={() => openEditForm(task)}>
                            编辑
                          </Button>
                          <Button
                            variant="secondary"
                            size="sm"
                            loading={triggerTask.isPending && triggerTask.variables?.id === task.id && !triggerTask.variables?.dryRun}
                            onClick={() => handleTrigger(task.id, false)}
                          >
                            触发
                          </Button>
                          <Button
                            variant="secondary"
                            size="sm"
                            loading={triggerTask.isPending && triggerTask.variables?.id === task.id && triggerTask.variables?.dryRun === true}
                            onClick={() => handleTrigger(task.id, true)}
                          >
                            试运行
                          </Button>
                          <Button variant="danger" size="sm" onClick={() => setDeleteTarget(task)}>
                            删除
                          </Button>
                        </div>
                        <div className="flex items-center gap-[var(--space-3)] text-[var(--text-xs)]">
                          <button
                            type="button"
                            className="text-[var(--color-text-secondary)] hover:text-[var(--color-text)] underline cursor-pointer bg-transparent border-none p-0"
                            onClick={() => setExpandedLogs(isExpLogs ? null : task.id)}
                          >
                            {isExpLogs ? '收起日志' : '查看日志'}
                          </button>
                          <button
                            type="button"
                            className="text-[var(--color-text-secondary)] hover:text-[var(--color-text)] underline cursor-pointer bg-transparent border-none p-0"
                            onClick={() => setExpandedPreview(isExpPreview ? null : task.id)}
                          >
                            {isExpPreview ? '收起预览' : '试运行预览'}
                          </button>
                        </div>
                        {isExpLogs && <TaskLogsPanel taskId={task.id} />}
                        {isExpPreview && <DryRunPreviewPanel taskId={task.id} />}
                      </div>
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
            ? `确定要删除任务「${deleteTarget.name}」吗？此操作不可撤销。`
            : ''
        }
        confirmLabel="确认删除"
        cancelLabel="取消"
        danger
        loading={deleteTask.isPending}
        onConfirm={handleConfirmDelete}
        onCancel={() => {
          if (!deleteTask.isPending) setDeleteTarget(null)
        }}
      />
    </div>
  )
}
