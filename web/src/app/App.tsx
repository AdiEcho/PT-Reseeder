import { lazy, Suspense } from 'react'
import { BrowserRouter, Routes, Route } from 'react-router'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { Toaster } from 'sonner'
import AppLayout from './AppLayout'
import AuthGuard from './AuthGuard'
import { Spinner } from '../components/ui'

const DashboardPage = lazy(() => import('../features/dashboard/DashboardPage').then(m => ({ default: m.DashboardPage })))
const DownloadersPage = lazy(() => import('../features/downloaders/DownloadersPage'))
const FoldersPage = lazy(() => import('../features/folders/FoldersPage').then(m => ({ default: m.FoldersPage })))
const LoginPage = lazy(() => import('../features/login/LoginPage'))
const LogsPage = lazy(() => import('../features/logs/LogsPage'))
const RepostPage = lazy(() => import('../features/repost/RepostPage'))
const ReseedPage = lazy(() => import('../features/reseed/ReseedPage'))
const SettingsPage = lazy(() => import('../features/settings/SettingsPage').then(m => ({ default: m.SettingsPage })))
const SiteDetailPage = lazy(() => import('../features/sites/SiteDetailPage'))
const SitesPage = lazy(() => import('../features/sites/SitesPage'))
const TasksPage = lazy(() => import('../features/tasks/TasksPage'))

function PageLoadingFallback() {
  return (
    <div className="flex items-center justify-center min-h-[200px]">
      <Spinner size="lg" />
    </div>
  )
}

const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      retry: 1,
      refetchOnWindowFocus: false,
      staleTime: 30_000,
    },
  },
})

export default function App() {
  return (
    <QueryClientProvider client={queryClient}>
      <BrowserRouter>
        <Suspense fallback={<PageLoadingFallback />}>
          <Routes>
            {/* Public route — no layout */}
            <Route path="/login" element={<LoginPage />} />

            {/* Protected routes — inside auth guard + layout */}
            <Route
              element={
                <AuthGuard>
                  <AppLayout />
                </AuthGuard>
              }
            >
              <Route index element={<DashboardPage />} />
              <Route path="sites" element={<SitesPage />} />
              <Route path="sites/:id" element={<SiteDetailPage />} />
              <Route path="downloaders" element={<DownloadersPage />} />
              <Route path="folders" element={<FoldersPage />} />
              <Route path="tasks" element={<TasksPage />} />
              <Route path="reseed" element={<ReseedPage />} />
              <Route path="repost" element={<RepostPage />} />
              <Route path="logs" element={<LogsPage />} />
              <Route path="settings" element={<SettingsPage />} />
            </Route>
          </Routes>
        </Suspense>
        <Toaster
          position="top-right"
          richColors
          toastOptions={{
            style: {
              fontFamily: 'var(--font-body)',
              fontSize: '0.875rem',
            },
          }}
        />
      </BrowserRouter>
    </QueryClientProvider>
  )
}
