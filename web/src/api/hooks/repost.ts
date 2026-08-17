import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { api } from '../client'
import type {
  RepostEntry,
  RepostEntryResponse,
  ReviewRepostInput,
  AutofillResponse,
} from '../types'

export function useRepostQueue(status?: string) {
  const qs = status ? `?status=${encodeURIComponent(status)}` : ''
  return useQuery({
    queryKey: ['repost-queue', status],
    queryFn: () => api.get<RepostEntry[]>(`/api/repost/queue${qs}`),
  })
}

export function useReviewRepost() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: ({ id, ...input }: { id: number } & ReviewRepostInput) =>
      api.post<RepostEntryResponse>(`/api/repost/queue/${id}/review`, input),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['repost-queue'] })
    },
  })
}

export function useAutofillRepost() {
  return useMutation({
    mutationFn: (id: number) =>
      api.post<AutofillResponse>(`/api/repost/queue/${id}/autofill`),
  })
}

export function useSubmitRepost() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (id: number) =>
      api.post<RepostEntryResponse>(`/api/repost/queue/${id}/submit`),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['repost-queue'] })
    },
  })
}

export function useDeleteRepost() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (id: number) => api.delete(`/api/repost/queue/${id}`),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['repost-queue'] })
    },
  })
}
