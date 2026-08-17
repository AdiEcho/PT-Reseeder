import { useState } from 'react'
import { NavLink, Outlet, useNavigate } from 'react-router'
import { useQueryClient } from '@tanstack/react-query'
import { useThemeStore } from '../stores/theme'
import { useAuthStore } from '../stores/auth'
import { api } from '../api/client'

// Inline SVG icons — minimal, 16x16 viewBox
const icons = {
  home: (
    <svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
      <path d="M2.5 6.5L8 2l5.5 4.5V13a1 1 0 01-1 1h-9a1 1 0 01-1-1V6.5z" />
    </svg>
  ),
  globe: (
    <svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
      <circle cx="8" cy="8" r="6" />
      <path d="M2 8h12M8 2c1.5 1.5 2.5 3.5 2.5 6s-1 4.5-2.5 6c-1.5-1.5-2.5-3.5-2.5-6s1-4.5 2.5-6z" />
    </svg>
  ),
  download: (
    <svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
      <path d="M8 2v8m0 0l-3-3m3 3l3-3M3 12v1.5h10V12" />
    </svg>
  ),
  folder: (
    <svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
      <path d="M2 4.5V12a1 1 0 001 1h10a1 1 0 001-1V6a1 1 0 00-1-1H8L6.5 3.5H3a1 1 0 00-1 1z" />
    </svg>
  ),
  list: (
    <svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
      <path d="M3 4h10M3 8h10M3 12h10" />
    </svg>
  ),
  refresh: (
    <svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
      <path d="M2.5 8a5.5 5.5 0 019.5-3.5M13.5 8a5.5 5.5 0 01-9.5 3.5" />
      <path d="M12 2v3h-3M4 11v3h3" />
    </svg>
  ),
  send: (
    <svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
      <path d="M14 2L7 9M14 2l-4 12-3-5-5-3 12-4z" />
    </svg>
  ),
  fileText: (
    <svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
      <path d="M9.5 2H4.5a1 1 0 00-1 1v10a1 1 0 001 1h7a1 1 0 001-1V5L9.5 2z" />
      <path d="M9.5 2v3h3M6 8h4M6 10.5h4" />
    </svg>
  ),
  settings: (
    <svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
      <circle cx="8" cy="8" r="2" />
      <path d="M13.5 8a5.5 5.5 0 00-.2-1.3l1.3-1-1.2-2-1.5.6a5.5 5.5 0 00-1.2-.7L10.3 2H7.7l-.4 1.6c-.4.2-.8.4-1.2.7l-1.5-.6-1.2 2 1.3 1A5.5 5.5 0 004.5 8c0 .4 0 .9.2 1.3l-1.3 1 1.2 2 1.5-.6c.4.3.8.5 1.2.7l.4 1.6h2.6l.4-1.6c.4-.2.8-.4 1.2-.7l1.5.6 1.2-2-1.3-1c.1-.4.2-.9.2-1.3z" />
    </svg>
  ),
  sun: (
    <svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
      <circle cx="8" cy="8" r="3" />
      <path d="M8 1.5v1M8 13.5v1M3.4 3.4l.7.7M11.9 11.9l.7.7M1.5 8h1M13.5 8h1M3.4 12.6l.7-.7M11.9 4.1l.7-.7" />
    </svg>
  ),
  moon: (
    <svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
      <path d="M13.5 8.5a5.5 5.5 0 01-7.5-7.5 6 6 0 107.5 7.5z" />
    </svg>
  ),
  logOut: (
    <svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
      <path d="M6 14H3.5a1 1 0 01-1-1V3a1 1 0 011-1H6M10.5 11.5L14 8l-3.5-3.5M14 8H6" />
    </svg>
  ),
  chevronLeft: (
    <svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
      <path d="M10 3L5 8l5 5" />
    </svg>
  ),
  chevronRight: (
    <svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
      <path d="M6 3l5 5-5 5" />
    </svg>
  ),
}

const navItems = [
  { to: '/', label: '仪表盘', icon: icons.home, end: true },
  { to: '/sites', label: '站点', icon: icons.globe },
  { to: '/downloaders', label: '下载器', icon: icons.download },
  { to: '/folders', label: '文件夹', icon: icons.folder },
  { to: '/tasks', label: '任务', icon: icons.list },
  { to: '/reseed', label: '辅种', icon: icons.refresh },
  { to: '/repost', label: '转种', icon: icons.send },
  { to: '/logs', label: '日志', icon: icons.fileText },
  { to: '/settings', label: '设置', icon: icons.settings },
]

export default function AppLayout() {
  const [collapsed, setCollapsed] = useState(false)
  const { theme, setTheme } = useThemeStore()
  const { clearUser } = useAuthStore()
  const navigate = useNavigate()
  const queryClient = useQueryClient()

  const handleLogout = async () => {
    try {
      await api.post('/api/auth/logout')
    } catch {
      // ignore
    }
    clearUser()
    queryClient.removeQueries({ queryKey: ['auth'] })
    navigate('/login', { replace: true })
  }

  const cycleTheme = () => {
    const next: Record<string, 'light' | 'dark' | 'system'> = {
      light: 'dark',
      dark: 'system',
      system: 'light',
    }
    setTheme(next[theme])
  }

  const themeIcon = theme === 'dark' ? icons.moon : icons.sun

  return (
    <div className="flex h-screen overflow-hidden" style={{ backgroundColor: 'var(--color-bg)' }}>
      {/* Sidebar */}
      <aside
        className="flex flex-col shrink-0 overflow-hidden transition-all"
        style={{
          width: collapsed ? '48px' : '200px',
          backgroundColor: 'var(--color-bg-elevated)',
          borderRight: '1px solid var(--color-border)',
          transitionDuration: 'var(--transition-normal)',
        }}
      >
        {/* Header */}
        <div
          className="flex items-center justify-between shrink-0"
          style={{ height: '40px', padding: '0 var(--space-4)' }}
        >
          {!collapsed && (
            <span
              className="font-semibold truncate"
              style={{ fontSize: 'var(--text-sm)', color: 'var(--color-text)' }}
            >
              PT-Reseeder
            </span>
          )}
          <button
            onClick={() => setCollapsed(!collapsed)}
            className="flex items-center justify-center shrink-0 rounded cursor-pointer"
            style={{
              width: '24px',
              height: '24px',
              color: 'var(--color-text-muted)',
              transition: 'color var(--transition-fast)',
            }}
            onMouseEnter={(e) => (e.currentTarget.style.color = 'var(--color-text)')}
            onMouseLeave={(e) => (e.currentTarget.style.color = 'var(--color-text-muted)')}
            title={collapsed ? '展开侧栏' : '收起侧栏'}
          >
            {collapsed ? icons.chevronRight : icons.chevronLeft}
          </button>
        </div>

        {/* Nav */}
        <nav className="flex-1 overflow-y-auto" style={{ padding: 'var(--space-2)' }}>
          {navItems.map((item) => (
            <NavLink
              key={item.to}
              to={item.to}
              end={item.end}
              className="flex items-center gap-2 rounded no-underline"
              style={({ isActive }) => ({
                padding: collapsed ? 'var(--space-3)' : 'var(--space-3) var(--space-4)',
                marginBottom: 'var(--space-1)',
                fontSize: 'var(--text-sm)',
                color: isActive ? 'var(--color-accent)' : 'var(--color-text-secondary)',
                backgroundColor: isActive ? 'var(--color-accent-subtle)' : 'transparent',
                borderRadius: 'var(--radius-sm)',
                transition: `background-color var(--transition-fast), color var(--transition-fast)`,
                justifyContent: collapsed ? 'center' : 'flex-start',
              })}
              onMouseEnter={(e) => {
                if (!e.currentTarget.getAttribute('aria-current')) {
                  e.currentTarget.style.backgroundColor = 'var(--color-bg-muted)'
                }
              }}
              onMouseLeave={(e) => {
                const isActive = e.currentTarget.getAttribute('aria-current') === 'page'
                e.currentTarget.style.backgroundColor = isActive ? 'var(--color-accent-subtle)' : 'transparent'
              }}
            >
              <span className="shrink-0 flex items-center">{item.icon}</span>
              {!collapsed && <span className="truncate">{item.label}</span>}
            </NavLink>
          ))}
        </nav>

        {/* Bottom actions */}
        <div
          className="flex shrink-0 border-t"
          style={{
            borderColor: 'var(--color-border)',
            padding: 'var(--space-3)',
            gap: 'var(--space-2)',
            justifyContent: collapsed ? 'center' : 'flex-start',
            flexDirection: collapsed ? 'column' : 'row',
            alignItems: 'center',
          }}
        >
          <button
            onClick={cycleTheme}
            className="flex items-center justify-center rounded cursor-pointer"
            style={{
              width: '28px',
              height: '28px',
              color: 'var(--color-text-muted)',
              backgroundColor: 'transparent',
              border: 'none',
              transition: 'color var(--transition-fast), background-color var(--transition-fast)',
              borderRadius: 'var(--radius-sm)',
            }}
            onMouseEnter={(e) => {
              e.currentTarget.style.color = 'var(--color-text)'
              e.currentTarget.style.backgroundColor = 'var(--color-bg-muted)'
            }}
            onMouseLeave={(e) => {
              e.currentTarget.style.color = 'var(--color-text-muted)'
              e.currentTarget.style.backgroundColor = 'transparent'
            }}
            title={`主题: ${theme === 'dark' ? '深色' : theme === 'light' ? '浅色' : '跟随系统'}`}
          >
            {themeIcon}
          </button>
          <button
            onClick={handleLogout}
            className="flex items-center justify-center rounded cursor-pointer"
            style={{
              width: '28px',
              height: '28px',
              color: 'var(--color-text-muted)',
              backgroundColor: 'transparent',
              border: 'none',
              transition: 'color var(--transition-fast), background-color var(--transition-fast)',
              borderRadius: 'var(--radius-sm)',
            }}
            onMouseEnter={(e) => {
              e.currentTarget.style.color = 'var(--color-error)'
              e.currentTarget.style.backgroundColor = 'var(--color-bg-muted)'
            }}
            onMouseLeave={(e) => {
              e.currentTarget.style.color = 'var(--color-text-muted)'
              e.currentTarget.style.backgroundColor = 'transparent'
            }}
            title="登出"
          >
            {icons.logOut}
          </button>
        </div>
      </aside>

      {/* Main content */}
      <main className="flex-1 overflow-auto" style={{ padding: 'var(--space-6)' }}>
        <Outlet />
      </main>
    </div>
  )
}
