import { useState } from 'react'
import { toast } from 'sonner'
import {
  useRepostQueue,
  useReviewRepost,
  useAutofillRepost,
  useSubmitRepost,
  useDeleteRepost,
} from '../../api/hooks/repost'
import type { RepostEntry, AutofillResponse } from '../../api/types'
import { ConfirmDialog, EmptyState, PageHeader, StatusBadge } from '../../components/shared'
import { DataTable, type Column } from '../../components/shared/DataTable'
import {
  Button,
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogFooter,
  Tabs,
  TabsList,
  TabsTrigger,
  Textarea,
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from '../../components/ui'
import { formatShortTime } from '../../lib/time'

type StatusFilter = '' | 'pending' | 'approved' | 'submitted' | 'failed' | 'rejected'

interface ReviewState {
  entry: RepostEntry
  action: 'approve' | 'reject'
  notes: string
}

const STATUS_TABS: { value: StatusFilter; label: string }[] = [
  { value: '', label: '全部' },
  { value: 'pending', label: '待审核' },
  { value: 'approved', label: '已批准' },
  { value: 'submitted', label: '已提交' },
  { value: 'failed', label: '失败' },
  { value: 'rejected', label: '已拒绝' },
]

function isTauriDesktop(): boolean {
  return typeof window !== 'undefined' &&
    ('__TAURI__' in window || '__TAURI_INTERNALS__' in window)
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

export default function RepostPage() {
  const [activeTab, setActiveTab] = useState<StatusFilter>('')
  const [reviewState, setReviewState] = useState<ReviewState | null>(null)
  const [deleteTarget, setDeleteTarget] = useState<RepostEntry | null>(null)
  const [rejectConfirmPending, setRejectConfirmPending] = useState(false)

  const queue = useRepostQueue(activeTab || undefined)
  const reviewRepost = useReviewRepost()
  const autofillRepost = useAutofillRepost()
  const submitRepost = useSubmitRepost()
  const deleteRepost = useDeleteRepost()

  const list = queue.data ?? []

  const handleReview = () => {
    if (!reviewState) return
    const { entry, action, notes } = reviewState

    if (action === 'reject') {
      setRejectConfirmPending(true)
      return
    }

    reviewRepost.mutate(
      { id: entry.id, action, notes: notes || undefined },
      {
        onSuccess: () => {
          toast.success('审核已通过')
          setReviewState(null)
        },
        onError: (err) => {
          toast.error(`审核失败：${formatApiError(err, '未知错误')}`)
        },
      },
    )
  }

  const handleRejectConfirm = () => {
    if (!reviewState) return
    const { entry, notes } = reviewState

    reviewRepost.mutate(
      { id: entry.id, action: 'reject', notes: notes || undefined },
      {
        onSuccess: () => {
          toast.success('已拒绝')
          setReviewState(null)
          setRejectConfirmPending(false)
        },
        onError: (err) => {
          toast.error(`拒绝失败：${formatApiError(err, '未知错误')}`)
          setRejectConfirmPending(false)
        },
      },
    )
  }

  const handleAutofill = async (entry: RepostEntry) => {
    if (isTauriDesktop()) {
      try {
        const { invoke } = await import('@tauri-apps/api/core')
        const available = await invoke<boolean>('check_autofill_available')
        if (!available) {
          toast.error('自动填充功能当前不可用')
          return
        }
        const result = await invoke<AutofillResponse>('inject_repost_autofill', { entryId: entry.id })
        handleAutofillResult(result)
      } catch (err) {
        toast.error(`自动填充失败：${formatApiError(err, '未知错误')}`)
      }
    } else {
      autofillRepost.mutate(entry.id, {
        onSuccess: (result) => {
          handleAutofillResult(result)
        },
        onError: (err) => {
          toast.error(`自动填充失败：${formatApiError(err, '未知错误')}`)
        },
      })
    }
  }

  const handleAutofillResult = (result: AutofillResponse) => {
    if (result.success) {
      const filledStr = result.filled.length > 0 ? `已填充: ${result.filled.join(', ')}` : ''
      const skippedStr = result.skipped.length > 0 ? `跳过: ${result.skipped.join(', ')}` : ''
      const detail = [filledStr, skippedStr].filter(Boolean).join('；')
      toast.success(detail || result.message)
    } else {
      toast.error(result.message)
    }
    if (result.confirmation_required) {
      toast.info('需要进一步确认，请查看目标站点')
    }
  }

  const handleSubmit = (entry: RepostEntry) => {
    submitRepost.mutate(entry.id, {
      onSuccess: () => {
        toast.success('提交成功')
      },
      onError: (err) => {
        toast.error(`提交失败：${formatApiError(err, '未知错误')}`)
      },
    })
  }

  const handleConfirmDelete = () => {
    if (!deleteTarget) return
    deleteRepost.mutate(deleteTarget.id, {
      onSuccess: () => {
        toast.success('已删除')
        setDeleteTarget(null)
      },
      onError: (err) => {
        toast.error(`删除失败：${formatApiError(err, '未知错误')}`)
      },
    })
  }

  const handleOpenUploadPage = async (entry: RepostEntry) => {
    try {
      const { invoke } = await import('@tauri-apps/api/core')
      await invoke('open_upload_page', { entryId: entry.id })
    } catch (err) {
      toast.error(`打开上传页失败：${formatApiError(err, '未知错误')}`)
    }
  }

  const columns: Column<RepostEntry>[] = [
    {
      key: 'route',
      header: '来源站点→目标站点',
      render: (entry) => (
        <span className="whitespace-nowrap">{entry.source_site_name} → {entry.target_site_name}</span>
      ),
    },
    {
      key: 'source_torrent_id',
      header: '种子ID',
      render: (entry) => <span className="whitespace-nowrap">{entry.source_torrent_id}</span>,
    },
    {
      key: 'status',
      header: '状态',
      render: (entry) => <StatusBadge domain="repost" status={entry.status} />,
    },
    {
      key: 'review_notes',
      header: '备注',
      render: (entry) => (
        <span className="text-muted-foreground max-w-[200px] truncate block">
          {entry.review_notes || '—'}
        </span>
      ),
    },
    {
      key: 'created_at',
      header: '时间',
      render: (entry) => (
        <span className="text-muted-foreground whitespace-nowrap">{formatShortTime(entry.created_at)}</span>
      ),
    },
    {
      key: 'actions',
      header: '操作',
      render: (entry) => (
        <TooltipProvider>
          <div className="flex items-center gap-1">
            {entry.status === 'pending' && (
              <>
                <Tooltip>
                  <TooltipTrigger asChild>
                    <Button
                      variant="secondary"
                      size="sm"
                      onClick={(e) => { e.stopPropagation(); setReviewState({ entry, action: 'approve', notes: '' }) }}
                    >
                      审核
                    </Button>
                  </TooltipTrigger>
                  <TooltipContent>审核此转种条目</TooltipContent>
                </Tooltip>
                <Tooltip>
                  <TooltipTrigger asChild>
                    <Button
                      variant="secondary"
                      size="sm"
                      loading={autofillRepost.isPending && autofillRepost.variables === entry.id}
                      onClick={(e) => { e.stopPropagation(); handleAutofill(entry) }}
                    >
                      自动填充
                    </Button>
                  </TooltipTrigger>
                  <TooltipContent>自动填充转种信息</TooltipContent>
                </Tooltip>
              </>
            )}
            {entry.status === 'approved' && (
              <>
                <Tooltip>
                  <TooltipTrigger asChild>
                    <Button
                      variant="primary"
                      size="sm"
                      loading={submitRepost.isPending && submitRepost.variables === entry.id}
                      onClick={(e) => { e.stopPropagation(); handleSubmit(entry) }}
                    >
                      提交
                    </Button>
                  </TooltipTrigger>
                  <TooltipContent>提交到目标站点</TooltipContent>
                </Tooltip>
                <Tooltip>
                  <TooltipTrigger asChild>
                    <Button
                      variant="secondary"
                      size="sm"
                      loading={autofillRepost.isPending && autofillRepost.variables === entry.id}
                      onClick={(e) => { e.stopPropagation(); handleAutofill(entry) }}
                    >
                      自动填充
                    </Button>
                  </TooltipTrigger>
                  <TooltipContent>自动填充转种信息</TooltipContent>
                </Tooltip>
              </>
            )}
            {entry.status === 'submitted' && isTauriDesktop() && (
              <Tooltip>
                <TooltipTrigger asChild>
                  <Button
                    variant="secondary"
                    size="sm"
                    onClick={(e) => { e.stopPropagation(); handleOpenUploadPage(entry) }}
                  >
                    打开上传页
                  </Button>
                </TooltipTrigger>
                <TooltipContent>在浏览器中打开上传页面</TooltipContent>
              </Tooltip>
            )}
            <Tooltip>
              <TooltipTrigger asChild>
                <Button
                  variant="destructive"
                  size="sm"
                  onClick={(e) => { e.stopPropagation(); setDeleteTarget(entry) }}
                >
                  删除
                </Button>
              </TooltipTrigger>
              <TooltipContent>删除此转种条目</TooltipContent>
            </Tooltip>
          </div>
        </TooltipProvider>
      ),
    },
  ]

  return (
    <div>
      <PageHeader title="转种队列" />

      <p className="text-sm text-muted-foreground mb-4">
        管理转种条目的审核、自动填充与提交。
      </p>

      {/* Status Filter Tabs */}
      <Tabs value={activeTab} onValueChange={(v) => setActiveTab(v as StatusFilter)} className="mb-4">
        <TabsList>
          {STATUS_TABS.map((tab) => (
            <TabsTrigger key={tab.value} value={tab.value}>
              {tab.label}
            </TabsTrigger>
          ))}
        </TabsList>
      </Tabs>

      {queue.isError && (
        <div className="flex flex-col items-center justify-center py-6 gap-3">
          <p className="text-sm text-destructive m-0">
            加载转种队列失败：{formatApiError(queue.error, '未知错误')}
          </p>
          <Button variant="secondary" size="sm" onClick={() => queue.refetch()}>
            重试
          </Button>
        </div>
      )}

      {!queue.isError && (
        <DataTable
          columns={columns}
          data={list as unknown as Record<string, unknown>[]}
          loading={queue.isLoading}
          caption="转种队列"
          empty={
            <EmptyState
              title="暂无转种条目。"
              description="创建转种任务后，条目将在此处显示。"
            />
          }
        />
      )}

      {/* Review Dialog — uses standard Dialog */}
      <Dialog open={reviewState != null} onOpenChange={(v) => { if (!v && !reviewRepost.isPending) setReviewState(null) }}>
        <DialogContent hideClose>
          <DialogHeader>
            <DialogTitle>审核转种条目</DialogTitle>
          </DialogHeader>
          {reviewState && (
            <div className="flex flex-col gap-3 overflow-y-auto max-h-[60vh]">
              <p className="text-sm text-foreground m-0">
                {reviewState.entry.source_site_name} → {reviewState.entry.target_site_name}（种子 {reviewState.entry.source_torrent_id}）
              </p>
              <div className="flex flex-col gap-1">
                <label className="text-sm text-foreground/70">操作</label>
                <div className="flex gap-2">
                  <label className="inline-flex items-center gap-0.5 text-sm cursor-pointer">
                    <input
                      type="radio"
                      name="review-action"
                      checked={reviewState.action === 'approve'}
                      onChange={() => setReviewState((s) => s ? { ...s, action: 'approve' } : s)}
                    />
                    批准
                  </label>
                  <label className="inline-flex items-center gap-0.5 text-sm cursor-pointer">
                    <input
                      type="radio"
                      name="review-action"
                      checked={reviewState.action === 'reject'}
                      onChange={() => setReviewState((s) => s ? { ...s, action: 'reject' } : s)}
                    />
                    拒绝
                  </label>
                </div>
              </div>
              <Textarea
                label="备注"
                placeholder="可选备注..."
                value={reviewState.notes}
                onChange={(e) => setReviewState((s) => s ? { ...s, notes: e.target.value } : s)}
                className="min-h-[80px] resize-y"
              />
            </div>
          )}
          <DialogFooter>
            <Button
              variant="secondary"
              size="sm"
              onClick={() => setReviewState(null)}
              disabled={reviewRepost.isPending}
            >
              取消
            </Button>
            <Button
              variant="primary"
              size="sm"
              loading={reviewRepost.isPending}
              onClick={handleReview}
            >
              确认
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* Reject Confirmation */}
      <ConfirmDialog
        open={rejectConfirmPending}
        title="确认拒绝"
        message="确定拒绝？"
        confirmLabel="确认拒绝"
        cancelLabel="取消"
        danger
        loading={reviewRepost.isPending}
        onConfirm={handleRejectConfirm}
        onCancel={() => {
          if (!reviewRepost.isPending) setRejectConfirmPending(false)
        }}
      />

      {/* Delete Confirmation */}
      <ConfirmDialog
        open={deleteTarget != null}
        title="确认删除"
        message="确定要删除此转种条目吗？"
        confirmLabel="确认删除"
        cancelLabel="取消"
        danger
        loading={deleteRepost.isPending}
        onConfirm={handleConfirmDelete}
        onCancel={() => {
          if (!deleteRepost.isPending) setDeleteTarget(null)
        }}
      />
    </div>
  )
}
