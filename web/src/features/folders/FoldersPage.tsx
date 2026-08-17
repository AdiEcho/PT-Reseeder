import { useMemo, useState, type FormEvent } from 'react'
import { toast } from 'sonner'
import {
  useCreateFolder,
  useDeleteFolder,
  useDownloaders,
  useFolders,
} from '../../api/hooks'
import type { FolderInfo } from '../../api/types'
import {
  ConfirmDialog,
  EmptyState,
  LoadingSkeleton,
  PageHeader,
} from '../../components/shared'
import { Button, Input, Select } from '../../components/ui'
import { formatShortTime } from '../../lib/time'

const SCAN_MODE_OPTIONS = [
  { value: 'local', label: '本机磁盘' },
  { value: 'downloader', label: '从下载器读取' },
] as const

function scanModeLabel(mode: string): string {
  if (mode === 'local') return '本机磁盘'
  if (mode === 'downloader') return '从下载器读取'
  return mode
}

function downloaderLabel(
  downloaderId: number | undefined,
  names: Map<number, string>,
): string {
  if (downloaderId == null) return '-'
  const name = names.get(downloaderId)
  return name ? `${name} (#${downloaderId})` : `#${downloaderId}`
}

export function FoldersPage() {
  const foldersQuery = useFolders()
  const downloadersQuery = useDownloaders()
  const createFolder = useCreateFolder()
  const deleteFolder = useDeleteFolder()

  const [showForm, setShowForm] = useState(false)
  const [path, setPath] = useState('')
  const [scanMode, setScanMode] = useState('local')
  const [downloaderId, setDownloaderId] = useState('')
  const [pathError, setPathError] = useState<string>()
  const [dlError, setDlError] = useState<string>()
  const [formError, setFormError] = useState<string>()
  const [pendingDelete, setPendingDelete] = useState<Pick<FolderInfo, 'id' | 'path'> | null>(
    null,
  )

  const downloaderNames = useMemo(() => {
    const map = new Map<number, string>()
    for (const dl of downloadersQuery.data ?? []) {
      map.set(dl.id, dl.name)
    }
    return map
  }, [downloadersQuery.data])

  const downloaderOptions = useMemo(
    () => [
      { value: '', label: '请选择下载器' },
      ...(downloadersQuery.data ?? []).map((dl) => ({
        value: String(dl.id),
        label: `${dl.name} (#${dl.id})`,
      })),
    ],
    [downloadersQuery.data],
  )

  const resetForm = () => {
    setPath('')
    setDownloaderId('')
    setPathError(undefined)
    setDlError(undefined)
    setFormError(undefined)
  }

  const handleCreate = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault()
    setPathError(undefined)
    setDlError(undefined)
    setFormError(undefined)

    const trimmed = path.trim()
    if (!trimmed) {
      setPathError('文件夹路径不能为空。')
      return
    }

    let parsedDownloaderId: number | undefined
    if (scanMode === 'downloader') {
      const id = Number.parseInt(downloaderId.trim(), 10)
      if (!Number.isFinite(id) || id <= 0) {
        setDlError('请选择关联下载器。')
        return
      }
      parsedDownloaderId = id
    }

    createFolder.mutate(
      {
        path: trimmed,
        scan_mode: scanMode,
        ...(parsedDownloaderId != null ? { downloader_id: parsedDownloaderId } : {}),
      },
      {
        onSuccess: () => {
          toast.success('文件夹添加成功')
          resetForm()
          setShowForm(false)
        },
        onError: (error) => {
          const message = error instanceof Error ? error.message : String(error)
          toast.error(`添加失败：${message}`)
          setFormError(message)
        },
      },
    )
  }

  const handleConfirmDelete = () => {
    if (!pendingDelete) return
    const { id } = pendingDelete
    deleteFolder.mutate(id, {
      onSuccess: () => {
        toast.success('文件夹已删除')
        setPendingDelete(null)
      },
      onError: (error) => {
        const message = error instanceof Error ? error.message : String(error)
        toast.error(`删除失败：${message}`)
      },
    })
  }

  const folders = foldersQuery.data ?? []

  return (
    <div>
      <PageHeader
        title="文件夹管理"
        actions={
          <Button
            variant={showForm ? 'secondary' : 'primary'}
            onClick={() => setShowForm((open) => !open)}
          >
            {showForm ? '取消' : '添加文件夹'}
          </Button>
        }
      />

      {showForm && (
        <section className="mb-[var(--space-6)] border border-[var(--color-border)] rounded-[var(--radius-md)] p-[var(--space-5)]">
          <h2 className="text-[var(--text-base)] font-medium text-[var(--color-text)] mb-[var(--space-4)]">
            添加文件夹
          </h2>
          <form
            className="flex flex-wrap items-end gap-[var(--space-4)]"
            onSubmit={handleCreate}
          >
            <div className="min-w-[240px] flex-1">
              <Input
                label="路径 *"
                placeholder="/path/to/torrents"
                value={path}
                error={pathError}
                onChange={(event) => {
                  setPath(event.target.value)
                  setPathError(undefined)
                }}
              />
            </div>
            <div className="min-w-[180px]">
              <Select
                label="种子来源"
                value={scanMode}
                options={[...SCAN_MODE_OPTIONS]}
                onChange={(event) => {
                  setScanMode(event.target.value)
                  setDlError(undefined)
                }}
              />
            </div>
            {scanMode === 'downloader' && (
              <div className="min-w-[220px] flex-1">
                {downloadersQuery.isError ? (
                  <>
                    <Input
                      label="关联下载器 *"
                      type="number"
                      placeholder="下载器 ID（数字）"
                      value={downloaderId}
                      error={dlError}
                      onChange={(event) => {
                        setDownloaderId(event.target.value)
                        setDlError(undefined)
                      }}
                    />
                    <p className="mt-[var(--space-1)] text-[var(--text-xs)] text-[var(--color-text-muted)]">
                      {`下载器列表加载失败（${downloadersQuery.error instanceof Error ? downloadersQuery.error.message : String(downloadersQuery.error)}），可临时填写数字 ID。`}
                    </p>
                  </>
                ) : (
                  <>
                    <Select
                      label="关联下载器 *"
                      disabled={downloadersQuery.isPending}
                      value={downloaderId}
                      options={
                        downloadersQuery.isPending
                          ? [{ value: '', label: '加载下载器...' }]
                          : downloaderOptions
                      }
                      onChange={(event) => {
                        setDownloaderId(event.target.value)
                        setDlError(undefined)
                      }}
                    />
                    {dlError && (
                      <span className="mt-[var(--space-1)] text-[var(--text-xs)] text-[var(--color-error)]">
                        {dlError}
                      </span>
                    )}
                    <p className="mt-[var(--space-1)] text-[var(--text-xs)] text-[var(--color-text-muted)]">
                      从「下载器管理」中已配置的客户端里选择；列表显示名称与 ID。
                    </p>
                  </>
                )}
              </div>
            )}
            <Button type="submit" loading={createFolder.isPending}>
              {createFolder.isPending ? '添加中...' : '添加'}
            </Button>
          </form>
          {formError && (
            <p className="mt-[var(--space-3)] text-[var(--text-xs)] text-[var(--color-error)]">
              {formError}
            </p>
          )}
        </section>
      )}

      <section>
        <h2 className="text-[var(--text-base)] font-medium text-[var(--color-text)] mb-[var(--space-4)]">
          种子文件夹
        </h2>
        {foldersQuery.isPending ? (
          <LoadingSkeleton variant="table" rows={5} />
        ) : foldersQuery.isError ? (
          <div className="flex flex-col items-start gap-[var(--space-3)] py-[var(--space-6)]">
            <p className="text-[var(--text-sm)] text-[var(--color-error)]">
              文件夹加载失败：
              {foldersQuery.error instanceof Error
                ? foldersQuery.error.message
                : String(foldersQuery.error)}
            </p>
            <Button variant="secondary" size="sm" onClick={() => void foldersQuery.refetch()}>
              重试
            </Button>
          </div>
        ) : folders.length === 0 ? (
          <EmptyState icon="📁" title="尚未配置任何文件夹。" />
        ) : (
          <div className="overflow-x-auto border border-[var(--color-border)] rounded-[var(--radius-md)]">
            <table className="w-full border-collapse text-[var(--text-sm)]">
              <thead>
                <tr className="bg-[var(--color-bg-subtle)] border-b border-[var(--color-border)]">
                  {['路径', '种子来源', '下载器', '启用', '上次扫描', '操作'].map((header) => (
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
                {folders.map((folder) => (
                  <tr
                    key={folder.id}
                    className="border-b border-[var(--color-border-subtle)] last:border-b-0 hover:bg-[var(--color-bg-subtle)] transition-colors duration-[var(--transition-fast)]"
                  >
                    <td className="px-[var(--space-4)] h-7 text-[var(--color-text)] whitespace-nowrap">
                      {folder.path}
                    </td>
                    <td className="px-[var(--space-4)] h-7 text-[var(--color-text)] whitespace-nowrap">
                      {scanModeLabel(folder.scan_mode)}
                    </td>
                    <td className="px-[var(--space-4)] h-7 text-[var(--color-text-muted)] whitespace-nowrap">
                      {downloaderLabel(folder.downloader_id, downloaderNames)}
                    </td>
                    <td
                      className={[
                        'px-[var(--space-4)] h-7 whitespace-nowrap',
                        folder.enabled
                          ? 'text-[var(--color-success)]'
                          : 'text-[var(--color-text-muted)]',
                      ].join(' ')}
                    >
                      {folder.enabled ? '是' : '否'}
                    </td>
                    <td className="px-[var(--space-4)] h-7 text-[var(--color-text-muted)] whitespace-nowrap">
                      {formatShortTime(folder.last_scanned_at)}
                    </td>
                    <td className="px-[var(--space-4)] h-7 whitespace-nowrap">
                      <Button
                        variant="danger"
                        size="sm"
                        onClick={() => setPendingDelete({ id: folder.id, path: folder.path })}
                      >
                        删除
                      </Button>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </section>

      <ConfirmDialog
        open={pendingDelete != null}
        title="确认删除"
        message={
          pendingDelete
            ? `确定要删除文件夹「${pendingDelete.path}」吗？此操作不可撤销。`
            : ''
        }
        confirmLabel="确认删除"
        cancelLabel="取消"
        danger
        loading={deleteFolder.isPending}
        onConfirm={handleConfirmDelete}
        onCancel={() => {
          if (!deleteFolder.isPending) setPendingDelete(null)
        }}
      />
    </div>
  )
}

export default FoldersPage
