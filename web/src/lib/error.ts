/**
 * Shared API error formatting.
 * Previously duplicated across 6+ page files.
 */
export function formatApiError(error: unknown): string {
  if (error instanceof Error) return error.message
  if (typeof error === 'string') return error
  return '未知错误'
}
