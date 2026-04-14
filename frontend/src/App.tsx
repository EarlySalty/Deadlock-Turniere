import { Routes, Route } from 'react-router-dom'
import Layout from '@/components/layout/Layout'
import Home from '@/pages/Home'
import Tournament from '@/pages/Tournament'
import Login from '@/pages/Login'
import Admin from '@/pages/Admin'
import Hilfe from '@/pages/Hilfe'
import Leaderboard from '@/pages/Leaderboard'
import PlayerProfile from '@/pages/PlayerProfile'
import ProtectedRoute from '@/components/auth/ProtectedRoute'

export default function App() {
  return (
    <Routes>
      <Route element={<Layout />}>
        <Route index element={<Home />} />
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
  )
}
