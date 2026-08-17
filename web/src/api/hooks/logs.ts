import { useQuery } from '@tanstack/react-query'
import { api } from '../client'
import type { LogFileInfo, LogPage, LogQueryParams } from '../types'

export function useLogFiles() {
  return useQuery({
    queryKey: ['logs', 'files'],
    queryFn: () => api.get<LogFileInfo[]>('/api/logs/files'),
  })
}

export function useLogs(params: LogQueryParams) {
  const searchParams = new URLSearchParams()
  if (params.filename) searchParams.set('filename', params.filename)
  if (params.page != null) searchParams.set('page', String(params.page))
  if (params.page_size != null) searchParams.set('page_size', String(params.page_size))
  if (params.level) searchParams.set('level', params.level)
  if (params.keyword) searchParams.set('keyword', params.keyword)
  if (params.task_id != null) searchParams.set('task_id', String(params.task_id))
  const qs = searchParams.toString()

  return useQuery({
    queryKey: ['logs', params],
    queryFn: () => api.get<LogPage>(`/api/logs${qs ? `?${qs}` : ''}`),
  })
}
