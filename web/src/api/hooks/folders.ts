import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { api } from '../client'
import type { FolderInfo, CreateFolderInput } from '../types'

export function useFolders() {
  return useQuery({
    queryKey: ['folders'],
    queryFn: () => api.get<FolderInfo[]>('/api/folders'),
    staleTime: 60_000,
  })
}

export function useCreateFolder() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (input: CreateFolderInput) =>
      api.post<FolderInfo>('/api/folders', input),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['folders'] })
    },
  })
}

export function useDeleteFolder() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (id: number) => api.delete(`/api/folders/${id}`),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['folders'] })
    },
  })
}
