import { lazy, Suspense } from 'react'
import { Routes, Route } from 'react-router-dom'
import Layout from '@/components/layout/Layout'
import Home from '@/pages/Home'
import DraftLobbyNeu from '@/pages/DraftLobbyNeu'
import ProtectedRoute from '@/components/auth/ProtectedRoute'

const Tournament = lazy(() => import('@/pages/Tournament'))
const Login = lazy(() => import('@/pages/Login'))
const Admin = lazy(() => import('@/pages/Admin'))
const Hilfe = lazy(() => import('@/pages/Hilfe'))
const Leaderboard = lazy(() => import('@/pages/Leaderboard'))
const PlayerProfile = lazy(() => import('@/pages/PlayerProfile'))
const DraftBoard = lazy(() => import('@/pages/DraftBoard'))
const CompFinder = lazy(() => import('@/pages/CompFinder'))

export default function App() {
  return (
    <Suspense fallback={<div role="status" className="mx-auto max-w-7xl px-6 py-12 text-sm text-muted">Ansicht wird geladen …</div>}>
      <Routes>
        <Route element={<Layout />}>
          <Route index element={<Home />} />
          {/* Vor der :id-Route: sonst liest der Router "draft" als Turnier-ID und
              zeigt eine leere Arena. React Router rankt statische Segmente zwar
              hoeher, aber die Reihenfolge hier macht es fuer den Leser eindeutig. */}
          <Route path="draft" element={<DraftLobbyNeu />} />
          <Route path="draft/:code" element={<DraftBoard />} />
          <Route path="comp" element={<CompFinder />} />
          <Route path="comp/:code" element={<CompFinder />} />
          <Route path=":id" element={<Tournament />} />
          <Route path="hilfe" element={<Hilfe />} />
          <Route path="login" element={<Login />} />
          <Route path="rangliste" element={<Leaderboard />} />
          <Route path="spieler/:username" element={<PlayerProfile />} />
          <Route path="profil" element={
            <ProtectedRoute>
              <PlayerProfile />
            </ProtectedRoute>
          } />
          <Route path="admin" element={
            <ProtectedRoute requireMod>
              <Admin />
            </ProtectedRoute>
          } />
        </Route>
      </Routes>
    </Suspense>
  )
}
