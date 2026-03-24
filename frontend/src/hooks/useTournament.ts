import { useQuery } from '@tanstack/react-query'
import { fetchTournaments, fetchTournament } from '@/api/client'

export function useTournaments() {
  return useQuery({
    queryKey: ['tournaments'],
    queryFn: fetchTournaments,
  })
}

export function useTournament(id: number) {
  return useQuery({
    queryKey: ['tournaments', id],
    queryFn: () => fetchTournament(id),
    enabled: id > 0,
  })
}
