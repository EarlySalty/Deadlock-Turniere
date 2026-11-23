import { Suspense } from 'react'
import { Outlet, useLocation } from 'react-router-dom'
import Header from './Header'

export default function Layout() {
  const { pathname } = useLocation()
  const isDraft = pathname === '/draft' || pathname.startsWith('/draft/')
  const isAdmin = pathname.startsWith('/admin')

  return (
    <div className="min-h-screen bg-background text-foreground">
      <Header />
      <main
        id="main-content"
        className={isDraft
          ? 'mx-auto w-full max-w-none px-0 py-0'
          : isAdmin
            ? 'mx-auto w-full max-w-[1680px] px-4 py-6 sm:px-6 xl:px-8'
            : 'mx-auto w-full max-w-[1560px] px-4 py-6 sm:px-6 xl:px-8'}
      >
        <Suspense fallback={<div role="status" className="mx-auto max-w-7xl px-6 py-12 text-sm text-muted">Ansicht wird geladen …</div>}>
          <Outlet />
        </Suspense>
      </main>
    </div>
  )
}
