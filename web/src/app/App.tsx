import { BrowserRouter, Routes, Route } from 'react-router'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { Toaster } from 'sonner'
import AppLayout from './AppLayout'
import AuthGuard from './AuthGuard'
import { DashboardPage } from '../features/dashboard/DashboardPage'
import DownloadersPage from '../features/downloaders/DownloadersPage'
import { FoldersPage } from '../features/folders/FoldersPage'
import LoginPage from '../features/login/LoginPage'
import LogsPage from '../features/logs/LogsPage'
import RepostPage from '../features/repost/RepostPage'
import ReseedPage from '../features/reseed/ReseedPage'
import { SettingsPage } from '../features/settings/SettingsPage'
import SiteDetailPage from '../features/sites/SiteDetailPage'
import SitesPage from '../features/sites/SitesPage'
import TasksPage from '../features/tasks/TasksPage'

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
