import { Outlet, useLocation } from 'react-router-dom'
import Header from './Header'

export default function Layout() {
  const isAdmin = useLocation().pathname.startsWith('/admin')
  return (
    <div className="min-h-screen bg-background text-foreground">
      <Header />
      <main className={`mx-auto w-full px-4 py-6 sm:px-6 lg:px-8 lg:py-8 ${isAdmin ? 'max-w-[1600px]' : 'max-w-7xl'}`}>
        <Outlet />
      </main>
    </div>
  )
}
