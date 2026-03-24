import { Routes, Route } from 'react-router-dom'
import Layout from '@/components/layout/Layout'
import Home from '@/pages/Home'
import Tournament from '@/pages/Tournament'
import Login from '@/pages/Login'
import Admin from '@/pages/Admin'
import ProtectedRoute from '@/components/auth/ProtectedRoute'

export default function App() {
  return (
    <Routes>
      <Route element={<Layout />}>
        <Route index element={<Home />} />
        <Route path="turnier/:id" element={<Tournament />} />
        <Route path="login" element={<Login />} />
        <Route path="admin" element={
          <ProtectedRoute requireMod>
            <Admin />
          </ProtectedRoute>
        } />
      </Route>
    </Routes>
  )
}
