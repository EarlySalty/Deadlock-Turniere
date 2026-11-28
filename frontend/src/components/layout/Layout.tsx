import { Suspense } from 'react'
import { Outlet, useLocation } from 'react-router-dom'
import Header from './Header'
import RouteErrorBoundary from './RouteErrorBoundary'

export default function Layout() {
  const { pathname } = useLocation()
  const isDraft = pathname === '/draft' || pathname.startsWith('/draft/')
  const isAdmin = pathname.startsWith('/admin')
  const width = isDraft
    ? 'max-w-none px-0 py-0'
    : `px-4 py-6 sm:px-6 lg:px-8 lg:py-8 ${isAdmin ? 'max-w-[1600px]' : 'max-w-7xl'}`
  return (
    <div className="min-h-screen bg-background text-foreground">
      <Header />
      <main id="main-content" className={`mx-auto w-full ${width}`}>
        <RouteErrorBoundary key={pathname}>
        <Suspense fallback={<div role="status" className="mx-auto max-w-7xl px-6 py-12 text-sm text-muted">Ansicht wird geladen …</div>}>
          <Outlet />
        </Suspense>
        </RouteErrorBoundary>
      </main>
    </div>
  )
}
