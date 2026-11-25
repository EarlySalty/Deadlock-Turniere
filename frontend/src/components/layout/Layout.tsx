import { Outlet, useLocation } from 'react-router-dom'
import Header from './Header'

export default function Layout() {
  const isAdmin = useLocation().pathname.startsWith('/admin')
  const isPreview = import.meta.env.BASE_URL.startsWith('/turnier-preview/')
  return (
    <div className="min-h-screen bg-background text-foreground">
      {isPreview && (
        <div role="status" className="flex flex-wrap items-center justify-center gap-x-3 gap-y-1 border-b border-amber-400/50 bg-amber-500/15 px-4 py-2 text-center text-xs font-semibold text-amber-200 sm:text-sm">
          <span>TESTVERSION – noch nicht gemergt</span>
          <span className="font-normal">Achtung: Änderungen wirken auf echte Turnierdaten.</span>
          <a className="underline underline-offset-2 hover:text-white" href="/turnier/">
            Zur Live-Seite
          </a>
        </div>
      )}
      <Header />
      <main className={`mx-auto w-full px-4 py-6 sm:px-6 lg:px-8 lg:py-8 ${isAdmin ? 'max-w-[1600px]' : 'max-w-7xl'}`}>
        <Outlet />
      </main>
    </div>
  )
}
