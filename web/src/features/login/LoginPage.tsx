import { useState } from 'react'
import { useNavigate } from 'react-router'
import { useQueryClient } from '@tanstack/react-query'
import { api } from '../../api/client'
import { useAuthStore } from '../../stores/auth'

export default function LoginPage() {
  const [username, setUsername] = useState('')
  const [password, setPassword] = useState('')
  const [isRegister, setIsRegister] = useState(false)
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState('')
  const [checkingFirstUse, setCheckingFirstUse] = useState(true)

  const navigate = useNavigate()
  const queryClient = useQueryClient()
  const { setUser } = useAuthStore()

  // Detect first-time use (no users exist)
  useState(() => {
    api.get<{ has_user: boolean }>('/api/auth/has-user')
      .then((res) => {
        if (!res.has_user) setIsRegister(true)
      })
      .catch(() => {
        // Assume login mode on error
      })
      .finally(() => setCheckingFirstUse(false))
  })

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    setError('')
    setLoading(true)

    try {
      const endpoint = isRegister ? '/api/auth/register' : '/api/auth/login'
      await api.post(endpoint, { username, password })
      // Cookie is set; fetch current user to confirm session
      const me = await api.get<{ username: string } | null>('/api/auth/me')
      const user = me ? { username: me.username } : { username }
      setUser(user)
      // Write directly into query cache so AuthGuard sees it immediately
      queryClient.setQueryData(['auth', 'me'], user)
      navigate('/', { replace: true })
    } catch (err) {
      setError(err instanceof Error ? err.message : '认证失败')
    } finally {
      setLoading(false)
    }
  }

  if (checkingFirstUse) {
    return (
      <div className="flex items-center justify-center h-screen bg-background">
        <div className="w-5 h-5 rounded-full border-2 border-border border-t-accent animate-spin" />
      </div>
    )
  }

  return (
    <div className="flex items-center justify-center h-screen bg-secondary/50">
      <div className="w-full max-w-[360px] bg-card border border-border rounded-xl p-8 shadow-lg">
        {/* Header */}
        <div className="text-center mb-6">
          <h1 className="text-xl font-semibold text-foreground tracking-tight font-body">
            ✦ PT-Reseeder
          </h1>
          <p className="mt-1.5 text-sm text-muted-foreground">
            {isRegister ? '创建管理员账号' : '登录以继续'}
          </p>
        </div>

        {/* Form */}
        <form onSubmit={handleSubmit} className="flex flex-col gap-4">
          <div className="flex flex-col gap-1.5">
            <label htmlFor="username" className="text-sm font-medium text-foreground">
              用户名
            </label>
            <input
              id="username"
              type="text"
              value={username}
              onChange={(e) => setUsername(e.target.value)}
              required
              autoComplete="username"
              autoFocus
              className="w-full h-9 px-3 text-sm text-foreground font-body bg-background border border-border rounded-md outline-none transition-colors duration-150 focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-1 placeholder:text-muted-foreground"
            />
          </div>

          <div className="flex flex-col gap-1.5">
            <label htmlFor="password" className="text-sm font-medium text-foreground">
              密码
            </label>
            <input
              id="password"
              type="password"
              value={password}
              onChange={(e) => setPassword(e.target.value)}
              required
              autoComplete={isRegister ? 'new-password' : 'current-password'}
              className="w-full h-9 px-3 text-sm text-foreground font-body bg-background border border-border rounded-md outline-none transition-colors duration-150 focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-1 placeholder:text-muted-foreground"
            />
          </div>

          {error && (
            <p className="text-sm text-destructive">{error}</p>
          )}

          <button
            type="submit"
            disabled={loading}
            className="w-full h-9 text-sm font-medium font-body text-primary-foreground bg-primary rounded-md cursor-pointer transition-colors duration-150 hover:bg-primary/90 disabled:opacity-50 disabled:pointer-events-none"
          >
            {loading ? '...' : isRegister ? '创建账号' : '登录'}
          </button>
        </form>
      </div>
    </div>
  )
}
