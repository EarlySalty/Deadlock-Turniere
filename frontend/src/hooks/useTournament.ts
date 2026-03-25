import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import {
  fetchTournaments, fetchTournament, fetchAdminTournaments, fetchAdminTournament,
  createTournament, updateTournament, deleteTournament,
  advanceTournament, assignRandomTeams,
  createTeam, joinTeam, signupSolo,
  generateGroups, generateBracket,
  createLobby, startMatch, fetchMatchResult, leaveLobby,
} from '@/api/client'
import type { TournamentCreate, TournamentUpdate } from '@/types/tournament'

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

export function useAdminTournaments() {
  return useQuery({
    queryKey: ['admin', 'tournaments'],
    queryFn: fetchAdminTournaments,
  })
}

export function useAdminTournament(id: number) {
  return useQuery({
    queryKey: ['admin', 'tournaments', id],
    queryFn: () => fetchAdminTournament(id),
    enabled: id > 0,
  })
}

export function useCreateTournament() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (data: TournamentCreate) => createTournament(data),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['tournaments'] })
      qc.invalidateQueries({ queryKey: ['admin', 'tournaments'] })
    },
  })
}

export function useUpdateTournament() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: ({ id, data }: { id: number; data: TournamentUpdate }) => updateTournament(id, data),
    onSuccess: (_data, vars) => {
      qc.invalidateQueries({ queryKey: ['tournaments'] })
      qc.invalidateQueries({ queryKey: ['tournaments', vars.id] })
      qc.invalidateQueries({ queryKey: ['admin', 'tournaments'] })
      qc.invalidateQueries({ queryKey: ['admin', 'tournaments', vars.id] })
    },
  })
}

export function useDeleteTournament() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (id: number) => deleteTournament(id),
    onSuccess: (_data, id) => {
      qc.invalidateQueries({ queryKey: ['tournaments'] })
      qc.invalidateQueries({ queryKey: ['admin', 'tournaments'] })
      qc.removeQueries({ queryKey: ['tournaments', id] })
      qc.removeQueries({ queryKey: ['admin', 'tournaments', id] })
    },
  })
}

export function useAdvanceTournament() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (id: number) => advanceTournament(id),
    onSuccess: (_data, id) => {
      qc.invalidateQueries({ queryKey: ['tournaments'] })
      qc.invalidateQueries({ queryKey: ['tournaments', id] })
      qc.invalidateQueries({ queryKey: ['admin', 'tournaments'] })
      qc.invalidateQueries({ queryKey: ['admin', 'tournaments', id] })
    },
  })
}

export function useAssignRandomTeams() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (id: number) => assignRandomTeams(id),
    onSuccess: (_data, id) => {
      qc.invalidateQueries({ queryKey: ['tournaments'] })
      qc.invalidateQueries({ queryKey: ['tournaments', id] })
      qc.invalidateQueries({ queryKey: ['admin', 'tournaments'] })
      qc.invalidateQueries({ queryKey: ['admin', 'tournaments', id] })
    },
  })
}

export function useCreateTeam() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: ({ tournamentId, name }: { tournamentId: number; name: string }) =>
      createTeam(tournamentId, name),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['tournaments'] }),
  })
}

export function useJoinTeam() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: ({ tournamentId, teamId }: { tournamentId: number; teamId: number }) =>
      joinTeam(tournamentId, teamId),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['tournaments'] }),
  })
}

export function useSignupSolo() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (tournamentId: number) => signupSolo(tournamentId),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['tournaments'] }),
  })
}

export function useGenerateGroups() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: ({ tournamentId, numGroups }: { tournamentId: number; numGroups?: number }) =>
      generateGroups(tournamentId, numGroups),
    onSuccess: (_data, vars) => {
      qc.invalidateQueries({ queryKey: ['tournaments'] })
      qc.invalidateQueries({ queryKey: ['tournaments', vars.tournamentId] })
      qc.invalidateQueries({ queryKey: ['admin', 'tournaments'] })
      qc.invalidateQueries({ queryKey: ['admin', 'tournaments', vars.tournamentId] })
    },
  })
}

export function useGenerateBracket() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (tournamentId: number) => generateBracket(tournamentId),
    onSuccess: (_data, tournamentId) => {
      qc.invalidateQueries({ queryKey: ['tournaments'] })
      qc.invalidateQueries({ queryKey: ['tournaments', tournamentId] })
      qc.invalidateQueries({ queryKey: ['admin', 'tournaments'] })
      qc.invalidateQueries({ queryKey: ['admin', 'tournaments', tournamentId] })
    },
  })
}

export function useCreateLobby() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: ({ tournamentId, matchId }: { tournamentId: number; matchId: number }) =>
      createLobby(tournamentId, matchId),
    onSuccess: (_data, vars) => {
      qc.invalidateQueries({ queryKey: ['tournaments'] })
      qc.invalidateQueries({ queryKey: ['tournaments', vars.tournamentId] })
      qc.invalidateQueries({ queryKey: ['admin', 'tournaments'] })
      qc.invalidateQueries({ queryKey: ['admin', 'tournaments', vars.tournamentId] })
    },
  })
}

export function useStartMatch() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: ({ tournamentId, matchId }: { tournamentId: number; matchId: number }) =>
      startMatch(tournamentId, matchId),
    onSuccess: (_data, vars) => {
      qc.invalidateQueries({ queryKey: ['tournaments'] })
      qc.invalidateQueries({ queryKey: ['tournaments', vars.tournamentId] })
      qc.invalidateQueries({ queryKey: ['admin', 'tournaments'] })
      qc.invalidateQueries({ queryKey: ['admin', 'tournaments', vars.tournamentId] })
    },
  })
}

export function useFetchMatchResult() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: ({ tournamentId, matchId }: { tournamentId: number; matchId: number }) =>
      fetchMatchResult(tournamentId, matchId),
    onSuccess: (_data, vars) => {
      qc.invalidateQueries({ queryKey: ['tournaments'] })
      qc.invalidateQueries({ queryKey: ['tournaments', vars.tournamentId] })
      qc.invalidateQueries({ queryKey: ['admin', 'tournaments'] })
      qc.invalidateQueries({ queryKey: ['admin', 'tournaments', vars.tournamentId] })
    },
  })
}

export function useLeaveLobby() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: ({ tournamentId, matchId }: { tournamentId: number; matchId: number }) =>
      leaveLobby(tournamentId, matchId),
    onSuccess: (_data, vars) => {
      qc.invalidateQueries({ queryKey: ['tournaments'] })
      qc.invalidateQueries({ queryKey: ['tournaments', vars.tournamentId] })
      qc.invalidateQueries({ queryKey: ['admin', 'tournaments'] })
      qc.invalidateQueries({ queryKey: ['admin', 'tournaments', vars.tournamentId] })
    },
  })
}
