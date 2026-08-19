import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { api } from '../client'
import type {
  TaskInfo,
  CreateTaskInput,
  TaskLogInfo,
  DryRunPreviewInfo,
  ReseedRunInfo,
  ReseedRunDetail,
} from '../types'

export function useTasks() {
  return useQuery({
    queryKey: ['tasks'],
    queryFn: () => api.get<TaskInfo[]>('/api/tasks'),
    staleTime: 30_000,
  })
}

export function useCreateTask() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (input: CreateTaskInput) =>
      api.post<TaskInfo>('/api/tasks', input),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['tasks'] })
    },
  })
}

export function useUpdateTask(id: number) {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (input: CreateTaskInput) =>
      api.put<TaskInfo>(`/api/tasks/${id}`, input),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['tasks'] })
    },
  })
}

export function useDeleteTask() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (id: number) => api.delete(`/api/tasks/${id}`),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['tasks'] })
    },
  })
}

export function useTriggerTask() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: ({ id, dryRun = false }: { id: number; dryRun?: boolean }) =>
      api.post(`/api/tasks/${id}/trigger?dry_run=${dryRun}`),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['tasks'] })
      queryClient.invalidateQueries({ queryKey: ['reseed-runs'] })
    },
  })
}

export function useTaskLogs(taskId: number) {
  return useQuery({
    queryKey: ['tasks', taskId, 'logs'],
    queryFn: () => api.get<TaskLogInfo[]>(`/api/tasks/${taskId}/logs`),
    enabled: taskId > 0,
    staleTime: 30_000,
  })
}

export function useDryRunPreview(taskId: number) {
  return useQuery({
    queryKey: ['tasks', taskId, 'dry-run-preview'],
    queryFn: () => api.get<DryRunPreviewInfo | null>(`/api/tasks/${taskId}/dry-run-preview`),
    enabled: taskId > 0,
    staleTime: 30_000,
  })
}

export function useReseedRuns(params?: { limit?: number; taskId?: number; refetchInterval?: number | false; refetchIntervalInBackground?: boolean }) {
  const searchParams = new URLSearchParams()
  if (params?.limit) searchParams.set('limit', String(params.limit))
  if (params?.taskId) searchParams.set('task_id', String(params.taskId))
  const qs = searchParams.toString()

  return useQuery({
    queryKey: ['reseed-runs', { limit: params?.limit, taskId: params?.taskId }],
    queryFn: () => api.get<ReseedRunInfo[]>(`/api/reseed-runs${qs ? `?${qs}` : ''}`),
    staleTime: 15_000,
    refetchInterval: params?.refetchInterval,
    refetchIntervalInBackground: params?.refetchIntervalInBackground,
  })
}

export function useReseedRunDetail(id: number) {
  return useQuery({
    queryKey: ['reseed-runs', id],
    queryFn: () => api.get<ReseedRunDetail | null>(`/api/reseed-runs/${id}`),
    enabled: id > 0,
    staleTime: 15_000,
  })
}
