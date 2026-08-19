import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { api } from '../client'
import type { ConfigEntry, UpdateConfigInput } from '../types'

export function useConfig() {
  return useQuery({
    queryKey: ['config'],
    queryFn: () => api.get<ConfigEntry[]>('/api/config'),
    staleTime: 300_000,
  })
}

export function useUpdateConfig() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (input: UpdateConfigInput) => api.put('/api/config', input),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['config'] })
    },
  })
}
