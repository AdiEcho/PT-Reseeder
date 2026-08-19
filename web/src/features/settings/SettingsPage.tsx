import { useEffect, useState } from 'react'
import { toast } from 'sonner'
import { useConfig, useUpdateConfig } from '../../api/hooks'
import type { ConfigEntry } from '../../api/types'
import { EmptyState, LoadingSkeleton, PageHeader } from '../../components/shared'
import { Button, Card, CardContent, CardHeader, CardTitle, Input, Label, Switch } from '../../components/ui'
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
      <Card>
        <CardHeader>
          <CardTitle>应用配置</CardTitle>
        </CardHeader>
        <CardContent>
          {entries.length === 0 ? (
            <EmptyState title="暂无设置项" description="还没有任何配置，可在下方添加。" />
          ) : (
            <div className="overflow-x-auto border border-border rounded-lg">
              <table className="w-full border-collapse text-sm">
                <caption className="sr-only">应用设置</caption>
                <thead>
                  <tr className="bg-muted border-b border-border">
                    <th scope="col" className="text-left px-4 py-1 text-xs font-medium text-foreground/70 whitespace-nowrap w-[22%]">
                      设置项
                    </th>
                    <th scope="col" className="text-left px-4 py-1 text-xs font-medium text-foreground/70">
                      值
                    </th>
                    <th scope="col" className="text-left px-4 py-1 text-xs font-medium text-foreground/70 whitespace-nowrap w-[140px]">
                      更新时间
                    </th>
                    <th scope="col" className="text-left px-4 py-1 text-xs font-medium text-foreground/70 whitespace-nowrap w-[72px]">
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
        </CardContent>
      </Card>

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
    <tr className="border-b border-border last:border-b-0 align-top hover:bg-muted">
      <td className="px-4 py-2">
        <div className="font-semibold text-foreground leading-tight">
          {labelForKey(entry.key)}
        </div>
        <div className="mt-0.5 text-xs text-muted-foreground font-mono">
          {entry.key}
        </div>
      </td>
      <td className="px-4 py-2">
        <div className="flex items-center gap-1">
          {isSeedingSize ? (
            <div className="inline-flex items-center gap-2">
              <Switch
                checked={value === 'true'}
                onCheckedChange={(checked) => {
                  setValue(checked ? 'true' : 'false')
                  setSaveError(null)
                }}
              />
              <Label className="text-sm cursor-pointer">
                {value === 'true' ? '已开启' : '已关闭'}
              </Label>
            </div>
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
          <div className="mt-0.5 text-xs text-muted-foreground">
            开启后，NexusPHP 用户信息刷新会额外请求一次当前做种列表。
          </div>
        )}
        {saveError && (
          <div className="mt-0.5 text-xs text-destructive">
            {saveError}
          </div>
        )}
      </td>
      <td className="px-4 py-2 text-muted-foreground whitespace-nowrap">
        {formatShortTime(entry.updated_at)}
      </td>
      <td className="px-4 py-2">
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
    <Card className="mt-5">
      <CardHeader>
        <CardTitle>添加设置项</CardTitle>
      </CardHeader>
      <CardContent>
        <p className="mb-3 text-xs text-muted-foreground leading-normal">
          可自由添加任意键名，但错误的键/值可能导致功能异常。建议仅添加你明确了解的配置项；未知键不会做
          schema 校验。
        </p>
        {addError && (
          <div className="mb-2 text-xs text-destructive">
            {addError}
          </div>
        )}
        <form
          className="flex flex-col sm:flex-row sm:flex-wrap items-stretch sm:items-end gap-2"
          onSubmit={(e) => {
            e.preventDefault()
            void handleAdd()
          }}
        >
          <div className="min-w-[200px] flex-1">
            <Input
              label="设置项名称"
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
              label="值"
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
      </CardContent>
    </Card>
  )
}

export default SettingsPage
