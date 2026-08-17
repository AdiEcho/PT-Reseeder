import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { api } from '../client'
import type { DownloaderInfo, CreateDownloaderInput, ToggleAutoStartInput } from '../types'

export function useDownloaders() {
  return useQuery({
    queryKey: ['downloaders'],
    queryFn: () => api.get<DownloaderInfo[]>('/api/downloaders'),
  })
}

export function useCreateDownloader() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (input: CreateDownloaderInput) =>
      api.post<DownloaderInfo>('/api/downloaders', input),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['downloaders'] })
    },
  })
}

export function useUpdateDownloader(id: number) {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (input: CreateDownloaderInput) =>
      api.put<DownloaderInfo>(`/api/downloaders/${id}`, input),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['downloaders'] })
    },
  })
}

export function useDeleteDownloader() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (id: number) => api.delete(`/api/downloaders/${id}`),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['downloaders'] })
    },
  })
}

export function useTestDownloader() {
  return useMutation({
    mutationFn: (id: number) => api.post<string>(`/api/downloaders/${id}/test`),
  })
}

export function useToggleAutoStart() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: ({ id, auto_start }: { id: number } & ToggleAutoStartInput) =>
      api.patch(`/api/downloaders/${id}/auto-start`, { auto_start }),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['downloaders'] })
    },
  })
}
