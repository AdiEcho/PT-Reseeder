import { useEffect, type ReactNode } from 'react'
import { Navigate } from 'react-router'
import { useQuery } from '@tanstack/react-query'
import { api } from '../api/client'
import { useAuthStore } from '../stores/auth'

interface MeResponse {
  username: string
}

export default function AuthGuard({ children }: { children: ReactNode }) {
  const { setUser } = useAuthStore()

  const { data, isLoading, isError } = useQuery({
    queryKey: ['auth', 'me'],
    queryFn: () => api.get<MeResponse | null>('/api/auth/me'),
    retry: false,
    staleTime: 60_000,
  })

  useEffect(() => {
    if (data) {
      setUser({ username: data.username })
    }
  }, [data, setUser])

  if (isLoading) {
    return (
      <div className="flex items-center justify-center h-screen" style={{ backgroundColor: 'var(--color-bg)' }}>
        <div className="flex flex-col items-center gap-3">
          <div
            className="w-6 h-6 rounded-full border-2 animate-spin"
            style={{
              borderColor: 'var(--color-border)',
              borderTopColor: 'var(--color-accent)',
            }}
          />
          <span className="text-xs" style={{ color: 'var(--color-text-muted)' }}>
            加载中...
          </span>
        </div>
      </div>
    )
  }

  if (isError || !data) {
    return <Navigate to="/login" replace />
  }

  return <>{children}</>
}
