import { useEffect, useState } from 'react'
import { NavLink, Outlet, useNavigate, useLocation } from 'react-router'
import { useQueryClient } from '@tanstack/react-query'
import {
  Home,
  Globe,
  Download,
  Folder,
  ListTodo,
  RefreshCw,
  Send,
  FileText,
  Settings,
  LogOut,
  ChevronLeft,
  ChevronRight,
  Sun,
  Moon,
  Menu,
} from 'lucide-react'
import { useThemeStore } from '../stores/theme'
import { useAuthStore } from '../stores/auth'
import { api } from '../api/client'
import { Button } from '../components/ui/Button'
import { Separator } from '../components/ui/Separator'
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from '../components/ui/Tooltip'
import { Sheet, SheetContent } from '../components/ui/Sheet'

const navItems = [
  { to: '/', label: '仪表盘', icon: Home, end: true },
  { to: '/sites', label: '站点', icon: Globe },
  { to: '/downloaders', label: '下载器', icon: Download },
  { to: '/folders', label: '文件夹', icon: Folder },
  { to: '/tasks', label: '任务', icon: ListTodo },
  { to: '/reseed', label: '辅种', icon: RefreshCw },
  { to: '/repost', label: '转种', icon: Send },
  { to: '/logs', label: '日志', icon: FileText },
  { to: '/settings', label: '设置', icon: Settings },
]

function useIsDesktop() {
  const [isDesktop, setIsDesktop] = useState(() =>
    typeof window !== 'undefined' ? window.matchMedia('(min-width: 768px)').matches : true,
  )

  useEffect(() => {
    const mq = window.matchMedia('(min-width: 768px)')
    const handler = (e: MediaQueryListEvent) => setIsDesktop(e.matches)
    mq.addEventListener('change', handler)
    return () => mq.removeEventListener('change', handler)
  }, [])

  return isDesktop
}

export default function AppLayout() {
  const [collapsed, setCollapsed] = useState(false)
  const [mobileOpen, setMobileOpen] = useState(false)
  const { theme, setTheme } = useThemeStore()
  const { clearUser } = useAuthStore()
  const navigate = useNavigate()
  const location = useLocation()
  const queryClient = useQueryClient()
  const isDesktop = useIsDesktop()

  // Close mobile sheet on navigation
  useEffect(() => {
    setMobileOpen(false)
  }, [location.pathname])

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

  const ThemeIcon = theme === 'dark' ? Moon : Sun
  const themeLabel = theme === 'dark' ? '深色' : theme === 'light' ? '浅色' : '跟随系统'

  const navContent = (
    <nav className="flex-1 overflow-y-auto px-2 py-1">
      {navItems.map((item) => (
        <NavLink
          key={item.to}
          to={item.to}
          end={item.end}
          className={({ isActive }) => [
            'flex items-center gap-2.5 rounded-md no-underline mb-0.5 transition-colors duration-150',
            'focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-1 outline-none',
            isDesktop && collapsed ? 'justify-center px-2 py-2' : 'px-3 py-2',
            isActive
              ? 'bg-accent/10 text-accent'
              : 'text-muted-foreground hover:bg-muted hover:text-foreground',
          ].join(' ')}
        >
          <span className="shrink-0 flex items-center">
            <item.icon size={16} />
          </span>
          {(isDesktop ? !collapsed : true) && <span className="text-sm truncate">{item.label}</span>}
        </NavLink>
      ))}
    </nav>
  )

  const bottomActions = (
    <div className="flex shrink-0 p-2 items-center justify-center gap-1">
      <Tooltip>
        <TooltipTrigger asChild>
          <Button
            variant="ghost"
            size="icon"
            className="w-8 h-8"
            onClick={cycleTheme}
            aria-label={`切换主题: ${themeLabel}`}
          >
            <ThemeIcon size={16} />
          </Button>
        </TooltipTrigger>
        <TooltipContent side="top">
          主题: {themeLabel}
        </TooltipContent>
      </Tooltip>
      <Tooltip>
        <TooltipTrigger asChild>
          <Button
            variant="ghost"
            size="icon"
            className="w-8 h-8 hover:text-destructive"
            onClick={handleLogout}
            aria-label="登出"
          >
            <LogOut size={16} />
          </Button>
        </TooltipTrigger>
        <TooltipContent side="top">登出</TooltipContent>
      </Tooltip>
    </div>
  )

  return (
    <TooltipProvider>
      <div className="flex flex-col md:flex-row h-screen overflow-hidden bg-background">
        {/* Mobile top bar */}
        <header className="md:hidden flex items-center h-12 px-4 border-b border-border bg-card">
          <Button variant="ghost" size="icon" onClick={() => setMobileOpen(true)} aria-label="打开导航菜单">
            <Menu size={20} />
          </Button>
          <span className="ml-3 text-sm font-semibold">PT-Reseeder</span>
        </header>

        {/* Mobile Sheet navigation */}
        <Sheet open={mobileOpen} onOpenChange={setMobileOpen}>
          <SheetContent side="left" className="flex flex-col p-0 w-[260px]">
            <div className="flex items-center h-12 px-4 shrink-0">
              <span className="text-sm font-semibold text-foreground tracking-tight">
                PT-Reseeder
              </span>
            </div>
            <Separator />
            {navContent}
            <Separator />
            {bottomActions}
          </SheetContent>
        </Sheet>

        {/* Desktop Sidebar */}
        {isDesktop && (
          <aside
            className="hidden md:flex flex-col shrink-0 overflow-hidden border-r border-border bg-card transition-all duration-200"
            style={{ width: collapsed ? '52px' : '210px' }}
          >
            {/* Header */}
            <div className="flex items-center justify-between h-12 px-3 shrink-0">
              {!collapsed && (
                <span className="text-sm font-semibold text-foreground tracking-tight truncate">
                  PT-Reseeder
                </span>
              )}
              <Button
                variant="ghost"
                size="icon"
                className="shrink-0 w-7 h-7"
                onClick={() => setCollapsed(!collapsed)}
                title={collapsed ? '展开侧栏' : '收起侧栏'}
                aria-label={collapsed ? '展开侧栏' : '收起侧栏'}
              >
                {collapsed ? <ChevronRight size={16} /> : <ChevronLeft size={16} />}
              </Button>
            </div>

            <Separator />

            {navContent}

            <Separator />

            {bottomActions}
          </aside>
        )}

        {/* Main content */}
        <main className="flex-1 overflow-auto p-4 md:p-6">
          <Outlet />
        </main>
      </div>
    </TooltipProvider>
  )
}
