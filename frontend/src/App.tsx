import { lazy } from 'react'
import { Routes, Route } from 'react-router-dom'
import Layout from '@/components/layout/Layout'
import ProtectedRoute from '@/components/auth/ProtectedRoute'

// Nur die tatsächlich geöffnete Ansicht laden. Das reduziert den initialen
// JS-Download insbesondere für Draft-Räume und mobile Verbindungen.
const Home = lazy(() => import('@/pages/Home'))
const DraftLobbyNeu = lazy(() => import('@/pages/DraftLobbyNeu'))
const Tournament = lazy(() => import('@/pages/Tournament'))
const Login = lazy(() => import('@/pages/Login'))
const Admin = lazy(() => import('@/pages/Admin'))
const Hilfe = lazy(() => import('@/pages/Hilfe'))
const Leaderboard = lazy(() => import('@/pages/Leaderboard'))
const PlayerProfile = lazy(() => import('@/pages/PlayerProfile'))
const DraftBoard = lazy(() => import('@/pages/DraftBoard'))
const CompFinder = lazy(() => import('@/pages/CompFinder'))

export default function App() {
  const isPreview = import.meta.env.BASE_URL.includes('/_preview/')
  return (
    <>
      {isPreview && (
        <div role="status" className="sticky top-0 z-[100] border-b border-[#c8a86b]/50 bg-[#191812] px-4 py-2 text-center text-xs font-semibold tracking-wide text-[#f3ddb3]">
          DDC-UI-VORSCHAU · Draft-Demo ohne echte Matches · Turnierdaten nur lesbar · Kein Produktiv-Deploy
        </div>
      )}
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
    </>
  )
}
