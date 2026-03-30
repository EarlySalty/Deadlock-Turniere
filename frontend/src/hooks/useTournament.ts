import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import {
  fetchTournaments, fetchTournament, fetchAdminTournaments, fetchAdminTournament,
  createTournament, updateTournament, deleteTournament,
  advanceTournament, assignRandomTeams, openCheckin, finalizeCheckin,
  createTeam, joinTeam, signupSolo, checkinPlayer, getCheckinStatus,
  withdrawSolo, kickMember, inviteSoloPlayer, leaveTeam,
  generateGroups, generateBracket,
  createLobby, startMatch, fetchMatchResult, leaveLobby, submitMatchResult,
  createAdminTeam, renameAdminTeam, deleteAdminTeam,
  changeAdminCaptain, removeAdminTeamMember, moveAdminTeamMember,
  assignSignupToAdminTeam, deleteAdminSignup, addTeamMember,
} from '@/api/client'
import type { ManualResult, TeamMoveRequest, TournamentCreate, TournamentUpdate } from '@/types/tournament'

function invalidateTournamentCaches(qc: ReturnType<typeof useQueryClient>, tournamentId?: number) {
  qc.invalidateQueries({ queryKey: ['tournaments'] })
  qc.invalidateQueries({ queryKey: ['admin', 'tournaments'] })
  if (typeof tournamentId === 'number') {
    qc.invalidateQueries({ queryKey: ['tournaments', tournamentId] })
    qc.invalidateQueries({ queryKey: ['admin', 'tournaments', tournamentId] })
    qc.invalidateQueries({ queryKey: ['tournaments', tournamentId, 'checkin-status'] })
  }
}

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
      invalidateTournamentCaches(qc)
    },
  })
}

export function useUpdateTournament() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: ({ id, data }: { id: number; data: TournamentUpdate }) => updateTournament(id, data),
    onSuccess: (_data, vars) => {
      invalidateTournamentCaches(qc, vars.id)
    },
  })
}

export function useDeleteTournament() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (id: number) => deleteTournament(id),
    onSuccess: (_data, id) => {
      invalidateTournamentCaches(qc, id)
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
      invalidateTournamentCaches(qc, id)
    },
  })
}

export function useOpenCheckin(tournamentId: number) {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: () => openCheckin(tournamentId),
    onSuccess: () => {
      invalidateTournamentCaches(qc, tournamentId)
    },
  })
}

export function useFinalizeCheckin(tournamentId: number) {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: ({
      confirm,
      allowedTeamIds,
      snapshotToken,
    }: {
      confirm?: boolean
      allowedTeamIds?: number[]
      snapshotToken?: string
    }) => finalizeCheckin(
      tournamentId,
      Boolean(confirm),
      allowedTeamIds ?? [],
      snapshotToken
    ),
    onSuccess: () => {
      invalidateTournamentCaches(qc, tournamentId)
    },
  })
}

export function useAssignRandomTeams() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (id: number) => assignRandomTeams(id),
    onSuccess: (_data, id) => {
      invalidateTournamentCaches(qc, id)
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

export function useCheckinStatus(tournamentId: number) {
  return useQuery({
    queryKey: ['tournaments', tournamentId, 'checkin-status'],
    queryFn: () => getCheckinStatus(tournamentId),
    enabled: tournamentId > 0,
    refetchInterval: 10_000,
  })
}

export function useCheckin(tournamentId: number) {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: () => checkinPlayer(tournamentId),
    onSuccess: () => {
      invalidateTournamentCaches(qc, tournamentId)
    },
  })
}

export function useGenerateGroups() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: ({ tournamentId, numGroups }: { tournamentId: number; numGroups?: number }) =>
      generateGroups(tournamentId, numGroups),
    onSuccess: (_data, vars) => {
      invalidateTournamentCaches(qc, vars.tournamentId)
    },
  })
}

export function useGenerateBracket() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (tournamentId: number) => generateBracket(tournamentId),
    onSuccess: (_data, tournamentId) => {
      invalidateTournamentCaches(qc, tournamentId)
    },
  })
}

export function useCreateLobby() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: ({ tournamentId, matchId }: { tournamentId: number; matchId: number }) =>
      createLobby(tournamentId, matchId),
    onSuccess: (_data, vars) => {
      invalidateTournamentCaches(qc, vars.tournamentId)
    },
  })
}

export function useStartMatch() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: ({ tournamentId, matchId }: { tournamentId: number; matchId: number }) =>
      startMatch(tournamentId, matchId),
    onSuccess: (_data, vars) => {
      invalidateTournamentCaches(qc, vars.tournamentId)
    },
  })
}

export function useFetchMatchResult() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: ({ tournamentId, matchId }: { tournamentId: number; matchId: number }) =>
      fetchMatchResult(tournamentId, matchId),
    onSuccess: (_data, vars) => {
      invalidateTournamentCaches(qc, vars.tournamentId)
    },
  })
}

export function useLeaveLobby() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: ({ tournamentId, matchId }: { tournamentId: number; matchId: number }) =>
      leaveLobby(tournamentId, matchId),
    onSuccess: (_data, vars) => {
      invalidateTournamentCaches(qc, vars.tournamentId)
    },
  })
}

export function useSubmitMatchResult() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: ({
      tournamentId,
      matchId,
      data,
      force,
    }: {
      tournamentId: number
      matchId: number
      data: ManualResult
      force?: boolean
    }) => submitMatchResult(tournamentId, matchId, data, { force }),
    onSuccess: (_data, vars) => {
      invalidateTournamentCaches(qc, vars.tournamentId)
    },
  })
}

export function useCreateAdminTeam() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: ({ tournamentId, name }: { tournamentId: number; name: string }) =>
      createAdminTeam(tournamentId, name),
    onSuccess: (_data, vars) => {
      invalidateTournamentCaches(qc, vars.tournamentId)
    },
  })
}

export function useRenameAdminTeam() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: ({ tournamentId, teamId, name }: { tournamentId: number; teamId: number; name: string }) =>
      renameAdminTeam(tournamentId, teamId, name),
    onSuccess: (_data, vars) => {
      invalidateTournamentCaches(qc, vars.tournamentId)
    },
  })
}

export function useDeleteAdminTeam() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: ({ tournamentId, teamId }: { tournamentId: number; teamId: number }) =>
      deleteAdminTeam(tournamentId, teamId),
    onSuccess: (_data, vars) => {
      invalidateTournamentCaches(qc, vars.tournamentId)
    },
  })
}

export function useChangeAdminCaptain() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: ({ tournamentId, teamId, discordId }: { tournamentId: number; teamId: number; discordId: string }) =>
      changeAdminCaptain(tournamentId, teamId, discordId),
    onSuccess: (_data, vars) => {
      invalidateTournamentCaches(qc, vars.tournamentId)
    },
  })
}

export function useRemoveAdminTeamMember() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: ({ tournamentId, teamId, discordId }: { tournamentId: number; teamId: number; discordId: string }) =>
      removeAdminTeamMember(tournamentId, teamId, discordId),
    onSuccess: (_data, vars) => {
      invalidateTournamentCaches(qc, vars.tournamentId)
    },
  })
}

export function useMoveAdminTeamMember() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: ({ tournamentId, targetTeamId, data }: { tournamentId: number; targetTeamId: number; data: TeamMoveRequest }) =>
      moveAdminTeamMember(tournamentId, targetTeamId, data),
    onSuccess: (_data, vars) => {
      invalidateTournamentCaches(qc, vars.tournamentId)
    },
  })
}

export function useAssignSignupToAdminTeam() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: ({ tournamentId, teamId, signupId }: { tournamentId: number; teamId: number; signupId: number }) =>
      assignSignupToAdminTeam(tournamentId, teamId, signupId),
    onSuccess: (_data, vars) => {
      invalidateTournamentCaches(qc, vars.tournamentId)
    },
  })
}

export function useDeleteAdminSignup() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: ({ tournamentId, signupId }: { tournamentId: number; signupId: number }) =>
      deleteAdminSignup(tournamentId, signupId),
    onSuccess: (_data, vars) => {
      invalidateTournamentCaches(qc, vars.tournamentId)
    },
  })
}

export function useAddMember(tournamentId: number) {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: ({
      teamId,
      discordId,
      discordName,
    }: {
      teamId: number
      discordId: string
      discordName: string
    }) => addTeamMember(tournamentId, teamId, discordId, discordName),
    onSuccess: () => {
      invalidateTournamentCaches(qc, tournamentId)
    },
  })
}

export function useWithdrawSolo(tournamentId: number) {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: () => withdrawSolo(tournamentId),
    onSuccess: () => {
      invalidateTournamentCaches(qc, tournamentId)
    },
  })
}

export function useKickMember(tournamentId: number) {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: ({ teamId, discordId }: { teamId: number; discordId: string }) =>
      kickMember(tournamentId, teamId, discordId),
    onSuccess: () => {
      invalidateTournamentCaches(qc, tournamentId)
    },
  })
}

export function useInviteSoloPlayer(tournamentId: number) {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: ({ teamId, discordId }: { teamId: number; discordId: string }) =>
      inviteSoloPlayer(tournamentId, teamId, discordId),
    onSuccess: () => {
      invalidateTournamentCaches(qc, tournamentId)
    },
  })
}

export function useLeaveTeam(tournamentId: number) {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: ({ teamId }: { teamId: number }) => leaveTeam(tournamentId, teamId),
    onSuccess: () => {
      invalidateTournamentCaches(qc, tournamentId)
    },
  })
}
