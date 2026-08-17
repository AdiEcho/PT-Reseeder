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
      setUser(me ? { username: me.username } : { username })
      queryClient.invalidateQueries({ queryKey: ['auth'] })
      navigate('/', { replace: true })
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Authentication failed')
    } finally {
      setLoading(false)
    }
  }

  if (checkingFirstUse) {
    return (
      <div className="flex items-center justify-center h-screen" style={{ backgroundColor: 'var(--color-bg)' }}>
        <div
          className="w-5 h-5 rounded-full border-2 animate-spin"
          style={{ borderColor: 'var(--color-border)', borderTopColor: 'var(--color-accent)' }}
        />
      </div>
    )
  }

  return (
    <div
      className="flex items-center justify-center h-screen"
      style={{ backgroundColor: 'var(--color-bg-subtle)' }}
    >
      <div
        className="w-full"
        style={{
          maxWidth: '340px',
          backgroundColor: 'var(--color-bg-elevated)',
          border: '1px solid var(--color-border)',
          borderRadius: 'var(--radius-lg)',
          padding: 'var(--space-8)',
          boxShadow: 'var(--shadow-lg)',
        }}
      >
        {/* Header */}
        <div className="text-center" style={{ marginBottom: 'var(--space-7)' }}>
          <h1
            className="font-semibold"
            style={{ fontSize: 'var(--text-xl)', color: 'var(--color-text)', margin: '0 0 var(--space-2)' }}
          >
            PT-Reseeder
          </h1>
          <p style={{ fontSize: 'var(--text-sm)', color: 'var(--color-text-muted)', margin: 0 }}>
            {isRegister ? 'Create your admin account' : 'Sign in to continue'}
          </p>
        </div>

        {/* Form */}
        <form onSubmit={handleSubmit} className="flex flex-col" style={{ gap: 'var(--space-5)' }}>
          <div className="flex flex-col" style={{ gap: 'var(--space-2)' }}>
            <label
              htmlFor="username"
              style={{ fontSize: 'var(--text-sm)', color: 'var(--color-text-secondary)' }}
            >
              Username
            </label>
            <input
              id="username"
              type="text"
              value={username}
              onChange={(e) => setUsername(e.target.value)}
              required
              autoComplete="username"
              autoFocus
              className="w-full outline-none"
              style={{
                height: '32px',
                padding: '0 var(--space-4)',
                fontSize: 'var(--text-base)',
                fontFamily: 'var(--font-sans)',
                color: 'var(--color-text)',
                backgroundColor: 'var(--color-bg)',
                border: '1px solid var(--color-border)',
                borderRadius: 'var(--radius-sm)',
                transition: 'border-color var(--transition-fast)',
              }}
              onFocus={(e) => (e.currentTarget.style.borderColor = 'var(--color-accent)')}
              onBlur={(e) => (e.currentTarget.style.borderColor = 'var(--color-border)')}
            />
          </div>

          <div className="flex flex-col" style={{ gap: 'var(--space-2)' }}>
            <label
              htmlFor="password"
              style={{ fontSize: 'var(--text-sm)', color: 'var(--color-text-secondary)' }}
            >
              Password
            </label>
            <input
              id="password"
              type="password"
              value={password}
              onChange={(e) => setPassword(e.target.value)}
              required
              autoComplete={isRegister ? 'new-password' : 'current-password'}
              className="w-full outline-none"
              style={{
                height: '32px',
                padding: '0 var(--space-4)',
                fontSize: 'var(--text-base)',
                fontFamily: 'var(--font-sans)',
                color: 'var(--color-text)',
                backgroundColor: 'var(--color-bg)',
                border: '1px solid var(--color-border)',
                borderRadius: 'var(--radius-sm)',
                transition: 'border-color var(--transition-fast)',
              }}
              onFocus={(e) => (e.currentTarget.style.borderColor = 'var(--color-accent)')}
              onBlur={(e) => (e.currentTarget.style.borderColor = 'var(--color-border)')}
            />
          </div>

          {error && (
            <p style={{ fontSize: 'var(--text-sm)', color: 'var(--color-error)', margin: 0 }}>
              {error}
            </p>
          )}

          <button
            type="submit"
            disabled={loading}
            className="w-full cursor-pointer font-medium"
            style={{
              height: '32px',
              fontSize: 'var(--text-sm)',
              fontFamily: 'var(--font-sans)',
              color: 'var(--color-accent-text)',
              backgroundColor: 'var(--color-accent)',
              border: 'none',
              borderRadius: 'var(--radius-sm)',
              transition: 'background-color var(--transition-fast)',
              opacity: loading ? 0.7 : 1,
            }}
            onMouseEnter={(e) => { if (!loading) e.currentTarget.style.backgroundColor = 'var(--color-accent-hover)' }}
            onMouseLeave={(e) => (e.currentTarget.style.backgroundColor = 'var(--color-accent)')}
          >
            {loading ? '...' : isRegister ? 'Create Account' : 'Sign In'}
          </button>
        </form>
      </div>
    </div>
  )
}
