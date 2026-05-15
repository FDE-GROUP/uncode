import { createRootRoute, createRoute, createRouter } from '@tanstack/react-router'
import { lazy } from 'react'
import { RootLayout } from './routes/__root'
import { HomePage } from './routes/home'
import { SessionsPage } from './routes/sessions'

const LazySessionDetailPage = lazy(() =>
  import('./routes/sessions.$id').then((m) => ({ default: m.SessionDetailPage })),
)
const LazyIssuesPage = lazy(() =>
  import('./routes/issues').then((m) => ({ default: m.IssuesPage })),
)
const LazyDashboardPage = lazy(() =>
  import('./routes/dashboard').then((m) => ({ default: m.DashboardPage })),
)
const LazySettingsPage = lazy(() =>
  import('./routes/settings').then((m) => ({ default: m.SettingsPage })),
)

const rootRoute = createRootRoute({ component: RootLayout })

const indexRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/',
  component: HomePage,
})

const sessionsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/sessions',
  component: SessionsPage,
})

const sessionDetailRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/sessions/$id',
  component: LazySessionDetailPage,
})

const issuesRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/issues',
  component: LazyIssuesPage,
})

const dashboardRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/dashboard',
  component: LazyDashboardPage,
})

const settingsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/settings',
  component: LazySettingsPage,
})

const routeTree = rootRoute.addChildren([
  indexRoute,
  sessionsRoute,
  sessionDetailRoute,
  issuesRoute,
  dashboardRoute,
  settingsRoute,
])

export const router = createRouter({ routeTree })

declare module '@tanstack/react-router' {
  interface Register {
    router: typeof router
  }
}
