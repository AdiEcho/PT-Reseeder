/**
 * Format a UTC ISO timestamp to local time.
 * Output: "YYYY-MM-DD HH:MM:SS.mmm"
 */
export function formatLocalTime(utcIso: string): string {
  const d = new Date(utcIso)
  if (isNaN(d.getTime())) {
    return utcIso // return as-is if invalid
  }

  const year = d.getFullYear()
  const month = String(d.getMonth() + 1).padStart(2, '0')
  const day = String(d.getDate()).padStart(2, '0')
  const hours = String(d.getHours()).padStart(2, '0')
  const minutes = String(d.getMinutes()).padStart(2, '0')
  const seconds = String(d.getSeconds()).padStart(2, '0')
  const millis = String(d.getMilliseconds()).padStart(3, '0')

  return `${year}-${month}-${day} ${hours}:${minutes}:${seconds}.${millis}`
}

/** Format a UTC timestamp for compact table cells (`YYYY-MM-DD HH:MM`). */
export function formatShortTime(value?: string | null): string {
  if (!value) return '—'
  const local = formatLocalTime(value)
  return local === value ? value.slice(0, 16) : local.slice(0, 16)
}

export function formatBytes(bytes: number): string {
  const kb = 1024
  const mb = kb * 1024
  const gb = mb * 1024
  const tb = gb * 1024
  const abs = Math.abs(bytes)
  if (abs >= tb) return `${(bytes / tb).toFixed(2)} TB`
  if (abs >= gb) return `${(bytes / gb).toFixed(2)} GB`
  if (abs >= mb) return `${(bytes / mb).toFixed(2)} MB`
  if (abs >= kb) return `${(bytes / kb).toFixed(2)} KB`
  return `${bytes} B`
}

export function formatDuration(seconds: number): string {
  const days = Math.floor(seconds / 86400)
  const hours = Math.floor((seconds % 86400) / 3600)
  if (days > 0) return `${days}d ${hours}h`
  if (hours > 0) return `${hours}h`
  return `${Math.floor(seconds / 60)}m`
}

/** Format a millisecond duration for reseed / task logs (`1.2 秒`). */
export function formatDurationMs(ms?: number | null): string {
  if (ms == null || ms < 0) return '—'
  return `${(ms / 1000).toFixed(1)} 秒`
}
