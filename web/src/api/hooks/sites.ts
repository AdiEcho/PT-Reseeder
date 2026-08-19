import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { api } from '../client'
import type {
  SiteInfo,
  SiteDetailData,
  CreateSiteInput,
  UpdateSiteInput,
  ValidateSiteInput,
  ValidateSiteResult,
  SiteDefinitionInfo,
} from '../types'

export function useSites() {
  return useQuery({
    queryKey: ['sites'],
    queryFn: () => api.get<SiteInfo[]>('/api/sites'),
    staleTime: 60_000,
  })
}

export function useSiteDetail(id: number) {
  return useQuery({
    queryKey: ['sites', id],
    queryFn: () => api.get<SiteDetailData>(`/api/sites/${id}`),
    enabled: id > 0,
    staleTime: 60_000,
  })
}

export function useCreateSite() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (input: CreateSiteInput) =>
      api.post<SiteInfo>('/api/sites', input),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['sites'] })
    },
  })
}

export function useUpdateSite(id: number) {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (input: UpdateSiteInput) =>
      api.put<SiteInfo>(`/api/sites/${id}`, input),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['sites'] })
      queryClient.invalidateQueries({ queryKey: ['sites', id] })
    },
  })
}

export function useDeleteSite() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (id: number) => api.delete(`/api/sites/${id}`),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['sites'] })
    },
  })
}

export function useValidateSite(id: number) {
  return useMutation({
    mutationFn: (input: ValidateSiteInput) =>
      api.post<ValidateSiteResult>(`/api/sites/${id}/validate`, input),
  })
}

export function useProbeSite() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (id: number) =>
      api.post<ValidateSiteResult>(`/api/sites/${id}/probe`),
    onSuccess: (_data, id) => {
      queryClient.invalidateQueries({ queryKey: ['sites'] })
      queryClient.invalidateQueries({ queryKey: ['sites', id] })
    },
  })
}

export function useRefreshSiteStats() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (id: number) => api.post(`/api/sites/${id}/refresh-stats`),
    onSuccess: (_data, id) => {
      queryClient.invalidateQueries({ queryKey: ['sites', id] })
    },
  })
}

export function useSiteDefinitions() {
  return useQuery({
    queryKey: ['site-definitions'],
    queryFn: () => api.get<SiteDefinitionInfo[]>('/api/site-definitions'),
    staleTime: 60_000,
  })
}
