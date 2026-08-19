import { useState } from 'react'
import { useNavigate } from 'react-router'
import { useQueryClient } from '@tanstack/react-query'
import { api } from '../../api/client'
import { useAuthStore } from '../../stores/auth'
import { Button, Card, CardContent, CardHeader, CardTitle, CardDescription, Input, Spinner } from '../../components/ui'

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
        <Spinner size="md" />
      </div>
    )
  }

  return (
    <div className="flex items-center justify-center h-screen bg-secondary/50">
      <Card className="w-full max-w-[360px] shadow-lg">
        <CardHeader className="text-center">
          <CardTitle className="text-xl font-semibold tracking-tight font-body">
            ✦ PT-Reseeder
          </CardTitle>
          <CardDescription>
            {isRegister ? '创建管理员账号' : '登录以继续'}
          </CardDescription>
        </CardHeader>

        <CardContent>
          <form onSubmit={handleSubmit} className="flex flex-col gap-4">
            <Input
              id="username"
              label="用户名"
              type="text"
              value={username}
              onChange={(e) => setUsername(e.target.value)}
              required
              autoComplete="username"
              autoFocus
            />

            <Input
              id="password"
              label="密码"
              type="password"
              value={password}
              onChange={(e) => setPassword(e.target.value)}
              required
              autoComplete={isRegister ? 'new-password' : 'current-password'}
            />

            {error && (
              <p className="text-sm text-destructive">{error}</p>
            )}

            <Button type="submit" disabled={loading} className="w-full">
              {loading ? '...' : isRegister ? '创建账号' : '登录'}
            </Button>
          </form>
        </CardContent>
      </Card>
    </div>
  )
}
