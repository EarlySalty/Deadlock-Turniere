import { useQuery, useQueryClient } from '@tanstack/react-query'
import { fetchMe } from '@/api/client'

export function useAuth() {
  const queryClient = useQueryClient()
  const { data: user, isLoading } = useQuery({
    queryKey: ['auth', 'me'],
    queryFn: fetchMe,
    retry: false,
    staleTime: 5 * 60_000,
  })
  const logout = () => {
    window.location.href = '/auth/discord/logout'
    queryClient.removeQueries({ queryKey: ['auth'] })
  }
  return { user, isLoading, isLoggedIn: !!user, logout }
}
