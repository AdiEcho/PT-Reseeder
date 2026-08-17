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
        <section className="mb-5 border border-border rounded-lg p-5">
          <h2 className="text-sm font-medium text-foreground mb-3">
            添加文件夹
          </h2>
          <form
            className="flex flex-wrap items-end gap-3"
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
                    <p className="mt-0.5 text-xs text-muted-foreground">
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
                      <span className="mt-0.5 text-xs text-destructive">
                        {dlError}
                      </span>
                    )}
                    <p className="mt-0.5 text-xs text-muted-foreground">
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
            <p className="mt-2 text-xs text-destructive">
              {formError}
            </p>
          )}
        </section>
      )}

      <section>
        <h2 className="text-sm font-medium text-foreground mb-3">
          种子文件夹
        </h2>
        {foldersQuery.isPending ? (
          <LoadingSkeleton variant="table" rows={5} />
        ) : foldersQuery.isError ? (
          <div className="flex flex-col items-start gap-2 py-5">
            <p className="text-sm text-destructive">
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
          <div className="overflow-x-auto border border-border rounded-lg">
            <table className="w-full border-collapse text-sm">
              <thead>
                <tr className="bg-muted border-b border-border">
                  {['路径', '种子来源', '下载器', '启用', '上次扫描', '操作'].map((header) => (
                    <th
                      key={header}
                      className="text-left px-4 h-7 text-xs font-medium text-foreground/70 whitespace-nowrap"
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
                    className="border-b border-border last:border-b-0 hover:bg-muted transition-colors duration-150"
                  >
                    <td className="px-4 h-7 text-foreground whitespace-nowrap">
                      {folder.path}
                    </td>
                    <td className="px-4 h-7 text-foreground whitespace-nowrap">
                      {scanModeLabel(folder.scan_mode)}
                    </td>
                    <td className="px-4 h-7 text-muted-foreground whitespace-nowrap">
                      {downloaderLabel(folder.downloader_id, downloaderNames)}
                    </td>
                    <td
                      className={[
                        'px-4 h-7 whitespace-nowrap',
                        folder.enabled
                          ? 'text-success'
                          : 'text-muted-foreground',
                      ].join(' ')}
                    >
                      {folder.enabled ? '是' : '否'}
                    </td>
                    <td className="px-4 h-7 text-muted-foreground whitespace-nowrap">
                      {formatShortTime(folder.last_scanned_at)}
                    </td>
                    <td className="px-4 h-7 whitespace-nowrap">
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
