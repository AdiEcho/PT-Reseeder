import { useEffect, useState } from 'react'
import { toast } from 'sonner'
import { useConfig, useUpdateConfig } from '../../api/hooks'
import type { ConfigEntry } from '../../api/types'
import { EmptyState, LoadingSkeleton, PageHeader } from '../../components/shared'
import { Button, Input } from '../../components/ui'
import { formatShortTime } from '../../lib/time'

const FETCH_SEEDING_SIZE_KEY = 'fetch_seeding_size'

const KNOWN_KEYS: Record<string, { label: string; secret: boolean }> = {
  jackett_url: { label: 'Jackett URL', secret: false },
  jackett_api_key: { label: 'Jackett API Key', secret: true },
  session_ttl_hours: { label: '会话有效期（小时）', secret: false },
  [FETCH_SEEDING_SIZE_KEY]: { label: '获取做种大小（额外请求）', secret: false },
  log_dir: { label: '日志目录', secret: false },
  log_retention_days: { label: '日志保留天数', secret: false },
  log_min_level: { label: '最低日志级别', secret: false },
}

function labelForKey(key: string): string {
  return KNOWN_KEYS[key]?.label ?? key
}

function isSecretKey(key: string): boolean {
  return KNOWN_KEYS[key]?.secret ?? false
}

export function SettingsPage() {
  const { data, isLoading, isError, error, refetch } = useConfig()
  const updateConfig = useUpdateConfig()

  const saveEntry = (key: string, value: string) =>
    updateConfig.mutateAsync({ key, value }).then(() => undefined)

  return (
    <div>
      <PageHeader title="设置" />

      {isLoading ? (
        <LoadingSkeleton variant="table" rows={6} />
      ) : isError ? (
        <EmptyState
          title="设置加载失败"
          description={error instanceof Error ? error.message : '无法获取应用配置'}
          actionLabel="重试"
          onAction={() => {
            void refetch()
          }}
        />
      ) : (
        <SettingsTable entries={data ?? []} onSave={saveEntry} />
      )}
    </div>
  )
}

function SettingsTable({
  entries,
  onSave,
}: {
  entries: ConfigEntry[]
  onSave: (key: string, value: string) => Promise<void>
}) {
  return (
    <section>
      <h2 className="mb-[var(--space-4)] text-[var(--text-lg)] font-semibold text-[var(--color-text)]">
        应用配置
      </h2>

      {entries.length === 0 ? (
        <div className="border border-[var(--color-border)] rounded-[var(--radius-md)]">
          <EmptyState title="暂无设置项" description="还没有任何配置，可在下方添加。" />
        </div>
      ) : (
        <div className="overflow-x-auto border border-[var(--color-border)] rounded-[var(--radius-md)]">
          <table className="w-full border-collapse text-[var(--text-sm)]">
            <thead>
              <tr className="bg-[var(--color-bg-subtle)] border-b border-[var(--color-border)]">
                <th className="text-left px-[var(--space-4)] py-[var(--space-2)] text-[var(--text-xs)] font-medium text-[var(--color-text-secondary)] whitespace-nowrap w-[22%]">
                  设置项
                </th>
                <th className="text-left px-[var(--space-4)] py-[var(--space-2)] text-[var(--text-xs)] font-medium text-[var(--color-text-secondary)]">
                  值
                </th>
                <th className="text-left px-[var(--space-4)] py-[var(--space-2)] text-[var(--text-xs)] font-medium text-[var(--color-text-secondary)] whitespace-nowrap w-[140px]">
                  更新时间
                </th>
                <th className="text-left px-[var(--space-4)] py-[var(--space-2)] text-[var(--text-xs)] font-medium text-[var(--color-text-secondary)] whitespace-nowrap w-[72px]">
                  操作
                </th>
              </tr>
            </thead>
            <tbody>
              {entries.map((entry) => (
                <SettingRow key={entry.key} entry={entry} onSave={onSave} />
              ))}
            </tbody>
          </table>
        </div>
      )}

      <AddSettingForm onSave={onSave} />
    </section>
  )
}

function SettingRow({
  entry,
  onSave,
}: {
  entry: ConfigEntry
  onSave: (key: string, value: string) => Promise<void>
}) {
  const isSecret = isSecretKey(entry.key)
  const isSeedingSize = entry.key === FETCH_SEEDING_SIZE_KEY
  const [value, setValue] = useState(entry.value)
  const [revealed, setRevealed] = useState(!isSecret)
  const [saving, setSaving] = useState(false)
  const [saveError, setSaveError] = useState<string | null>(null)

  useEffect(() => {
    setValue(entry.value)
  }, [entry.value])

  const handleSave = async () => {
    setSaving(true)
    setSaveError(null)
    try {
      await onSave(entry.key, value)
      toast.success('设置已保存')
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err)
      const text = `保存失败：${msg}`
      toast.error(text)
      setSaveError(text)
    } finally {
      setSaving(false)
    }
  }

  return (
    <tr className="border-b border-[var(--color-border-subtle)] last:border-b-0 align-top hover:bg-[var(--color-bg-subtle)]">
      <td className="px-[var(--space-4)] py-[var(--space-3)]">
        <div className="font-semibold text-[var(--color-text)] leading-[var(--leading-tight)]">
          {labelForKey(entry.key)}
        </div>
        <div className="mt-[var(--space-1)] text-[var(--text-xs)] text-[var(--color-text-muted)] font-mono">
          {entry.key}
        </div>
      </td>
      <td className="px-[var(--space-4)] py-[var(--space-3)]">
        <div className="flex items-center gap-[var(--space-2)]">
          {isSeedingSize ? (
            <label className="inline-flex items-center gap-[var(--space-2)] text-[var(--text-sm)] text-[var(--color-text)] cursor-pointer">
              <input
                type="checkbox"
                checked={value === 'true'}
                onChange={(e) => {
                  setValue(e.target.checked ? 'true' : 'false')
                  setSaveError(null)
                }}
                className="accent-[var(--color-accent)]"
              />
              <span>{value === 'true' ? '已开启' : '已关闭'}</span>
            </label>
          ) : (
            <Input
              type={revealed ? 'text' : 'password'}
              value={value}
              onChange={(e) => {
                setValue(e.target.value)
                setSaveError(null)
              }}
              className="w-full min-w-[160px]"
            />
          )}
          {isSecret && (
            <Button
              type="button"
              variant="secondary"
              size="sm"
              onClick={() => setRevealed((prev) => !prev)}
            >
              {revealed ? '隐藏' : '显示'}
            </Button>
          )}
        </div>
        {isSeedingSize && (
          <div className="mt-[var(--space-1)] text-[var(--text-xs)] text-[var(--color-text-muted)]">
            开启后，NexusPHP 用户信息刷新会额外请求一次当前做种列表。
          </div>
        )}
        {saveError && (
          <div className="mt-[var(--space-1)] text-[var(--text-xs)] text-[var(--color-error)]">
            {saveError}
          </div>
        )}
      </td>
      <td className="px-[var(--space-4)] py-[var(--space-3)] text-[var(--color-text-muted)] whitespace-nowrap">
        {formatShortTime(entry.updated_at)}
      </td>
      <td className="px-[var(--space-4)] py-[var(--space-3)]">
        <Button type="button" size="sm" loading={saving} onClick={() => void handleSave()}>
          {saving ? '保存中...' : '保存'}
        </Button>
      </td>
    </tr>
  )
}

function AddSettingForm({
  onSave,
}: {
  onSave: (key: string, value: string) => Promise<void>
}) {
  const [newKey, setNewKey] = useState('')
  const [newValue, setNewValue] = useState('')
  const [saving, setSaving] = useState(false)
  const [addError, setAddError] = useState<string | null>(null)

  const handleAdd = async () => {
    const key = newKey.trim()
    if (!key) {
      setAddError('设置项名称不能为空')
      return
    }
    if (key.includes(' ')) {
      setAddError('设置项名称不能包含空格')
      return
    }

    setSaving(true)
    setAddError(null)
    try {
      await onSave(key, newValue)
      toast.success('设置项已添加')
      setNewKey('')
      setNewValue('')
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err)
      const text = `添加失败：${msg}`
      toast.error(text)
      setAddError(text)
    } finally {
      setSaving(false)
    }
  }

  return (
    <div className="mt-[var(--space-6)] pt-[var(--space-5)] border-t border-[var(--color-border-subtle)]">
      <h3 className="mb-[var(--space-2)] text-[var(--text-base)] font-medium text-[var(--color-text)]">
        添加设置项
      </h3>
      <p className="mb-[var(--space-4)] text-[var(--text-xs)] text-[var(--color-text-muted)] leading-[var(--leading-normal)]">
        可自由添加任意键名，但错误的键/值可能导致功能异常。建议仅添加你明确了解的配置项；未知键不会做
        schema 校验。
      </p>
      {addError && (
        <div className="mb-[var(--space-3)] text-[var(--text-xs)] text-[var(--color-error)]">
          {addError}
        </div>
      )}
      <form
        className="flex flex-wrap items-end gap-[var(--space-3)]"
        onSubmit={(e) => {
          e.preventDefault()
          void handleAdd()
        }}
      >
        <div className="min-w-[200px] flex-1">
          <Input
            value={newKey}
            onChange={(e) => {
              setNewKey(e.target.value)
              setAddError(null)
            }}
            placeholder="设置项名称（如 jackett_url）"
          />
        </div>
        <div className="min-w-[200px] flex-1">
          <Input
            value={newValue}
            onChange={(e) => {
              setNewValue(e.target.value)
              setAddError(null)
            }}
            placeholder="值"
          />
        </div>
        <Button type="submit" loading={saving} disabled={!newKey.trim()}>
          {saving ? '添加中...' : '添加'}
        </Button>
      </form>
    </div>
  )
}

export default SettingsPage
