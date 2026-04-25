import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import {
  fetchTournaments, fetchTournament, fetchAdminTournaments, fetchAdminTournament,
  fetchMyTournamentStatus,
  createTournament, updateTournament, deleteTournament,
  advanceTournament, assignRandomTeams, openCheckin, revertCheckin, finalizeCheckin,
  createTeam, joinTeam, signupSolo, checkinPlayer, getCheckinStatus,
  withdrawSolo, kickMember, inviteBySignup, leaveTeam,
  generateGroups, generateBracket,
  triggerAutoLobby, fetchMiniGroups,
  createLobby, startMatch, fetchMatchResult, leaveLobby, submitMatchResult,
  createGroupLobby, startGroupMatch, fetchGroupMatchResult, leaveGroupLobby,
  resetMatch, setManualMatchLobby, resetGroupMatch, setManualGroupMatchLobby,
  fetchAvailableCasters, fetchMatchCasters, assignMatchCaster, removeMatchCaster,
  fetchMatchEventPresets, applyMatchConvars, applyMatchEventPreset,
  createAdminTeam, renameAdminTeam, deleteAdminTeam,
  changeAdminCaptain, removeAdminTeamMember, moveAdminTeamMember,
  assignSignupToAdminTeam, deleteAdminSignup, addTeamMember,
  setRecruitingStatus,
  fetchMyInvitations, acceptInvitation, rejectInvitation,
  applyToTeam, fetchTeamApplications, acceptApplication, rejectApplication,
  fetchConsent, setConsent, revokeConsent,
  fetchMyProfile, updateMyProfile, uploadProfileAvatar,
  fetchLeaderboard, fetchPlayerProfile,
  voiceMoveTeams, voiceMoveSammelpunkt, voiceMoveUser,
  fetchDraftHeroes, startDraft, fetchDraftSession, submitDraftAction,
  submitSeriesGameResult,
} from '@/api/client'
import type {
  ManualResult, TeamMoveRequest, TournamentCreate, TournamentUpdate,
  UserProfileUpdate,
} from '@/types/tournament'

function invalidateTournamentCaches(qc: ReturnType<typeof useQueryClient>, tournamentId?: number) {
  qc.invalidateQueries({ queryKey: ['tournaments'] })
  qc.invalidateQueries({ queryKey: ['admin', 'tournaments'] })
  if (typeof tournamentId === 'number') {
    qc.invalidateQueries({ queryKey: ['tournaments', tournamentId] })
    qc.invalidateQueries({ queryKey: ['admin', 'tournaments', tournamentId] })
    qc.invalidateQueries({ queryKey: ['tournaments', tournamentId, 'checkin-status'] })
    qc.invalidateQueries({ queryKey: ['tournaments', tournamentId, 'me'] })
    qc.invalidateQueries({ queryKey: ['tournaments', tournamentId, 'invitations'] })
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
    refetchInterval: 5_000,
  })
}

export function useMyTournamentStatus(tournamentId: number, enabled = true) {
  return useQuery({
    queryKey: ['tournaments', tournamentId, 'me'],
    queryFn: () => fetchMyTournamentStatus(tournamentId),
    enabled: tournamentId > 0 && enabled,
    refetchInterval: 5_000,
    retry: false,
  })
}

export function useAdminTournaments() {
  return useQuery({
    queryKey: ['admin', 'tournaments'],
    queryFn: fetchAdminTournaments,
    refetchInterval: 5_000,
  })
}

export function useAdminTournament(id: number) {
  return useQuery({
    queryKey: ['admin', 'tournaments', id],
    queryFn: () => fetchAdminTournament(id),
    enabled: id > 0,
    refetchInterval: 3_000,
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

export function useRevertCheckin(tournamentId: number) {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: () => revertCheckin(tournamentId),
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
    onSuccess: (_data, vars) => invalidateTournamentCaches(qc, vars.tournamentId),
  })
}

export function useJoinTeam() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: ({ tournamentId, teamId }: { tournamentId: number; teamId: number }) =>
      joinTeam(tournamentId, teamId),
    onSuccess: (_data, vars) => invalidateTournamentCaches(qc, vars.tournamentId),
  })
}

export function useSignupSolo() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (tournamentId: number) => signupSolo(tournamentId),
    onSuccess: (_data, tournamentId) => invalidateTournamentCaches(qc, tournamentId),
  })
}

export function useCheckinStatus(tournamentId: number) {
  return useQuery({
    queryKey: ['tournaments', tournamentId, 'checkin-status'],
    queryFn: () => getCheckinStatus(tournamentId),
    enabled: tournamentId > 0,
    refetchInterval: 3_000,
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

export function useTriggerAutoLobby() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (tournamentId: number) => triggerAutoLobby(tournamentId),
    onSuccess: (_data, tournamentId) => {
      invalidateTournamentCaches(qc, tournamentId)
    },
  })
}

export function useMiniGroups(tournamentId: number) {
  return useQuery({
    queryKey: ['admin', 'tournaments', tournamentId, 'mini-groups'],
    queryFn: () => fetchMiniGroups(tournamentId),
    enabled: tournamentId > 0,
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

export function useCreateGroupLobby() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: ({ tournamentId, matchId }: { tournamentId: number; matchId: number }) =>
      createGroupLobby(tournamentId, matchId),
    onSuccess: (_data, vars) => {
      invalidateTournamentCaches(qc, vars.tournamentId)
    },
  })
}

export function useStartGroupMatch() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: ({ tournamentId, matchId }: { tournamentId: number; matchId: number }) =>
      startGroupMatch(tournamentId, matchId),
    onSuccess: (_data, vars) => {
      invalidateTournamentCaches(qc, vars.tournamentId)
    },
  })
}

export function useFetchGroupMatchResult() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: ({ tournamentId, matchId }: { tournamentId: number; matchId: number }) =>
      fetchGroupMatchResult(tournamentId, matchId),
    onSuccess: (_data, vars) => {
      invalidateTournamentCaches(qc, vars.tournamentId)
    },
  })
}

export function useLeaveGroupLobby() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: ({ tournamentId, matchId }: { tournamentId: number; matchId: number }) =>
      leaveGroupLobby(tournamentId, matchId),
    onSuccess: (_data, vars) => {
      invalidateTournamentCaches(qc, vars.tournamentId)
    },
  })
}

export function useResetGroupMatch() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: ({ tournamentId, matchId }: { tournamentId: number; matchId: number }) =>
      resetGroupMatch(tournamentId, matchId),
    onSuccess: (_data, vars) => {
      invalidateTournamentCaches(qc, vars.tournamentId)
    },
  })
}

export function useSetManualGroupMatchLobby() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: ({
      tournamentId,
      matchId,
      partyCode,
      steamPartyId,
    }: {
      tournamentId: number
      matchId: number
      partyCode: string
      steamPartyId?: string
    }) => setManualGroupMatchLobby(tournamentId, matchId, { party_code: partyCode, steam_party_id: steamPartyId }),
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

export function useResetMatch() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: ({ tournamentId, matchId }: { tournamentId: number; matchId: number }) =>
      resetMatch(tournamentId, matchId),
    onSuccess: (_data, vars) => {
      invalidateTournamentCaches(qc, vars.tournamentId)
    },
  })
}

export function useSetManualMatchLobby() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: ({
      tournamentId,
      matchId,
      partyCode,
      steamPartyId,
    }: {
      tournamentId: number
      matchId: number
      partyCode: string
      steamPartyId?: string
    }) => setManualMatchLobby(tournamentId, matchId, { party_code: partyCode, steam_party_id: steamPartyId }),
    onSuccess: (_data, vars) => {
      invalidateTournamentCaches(qc, vars.tournamentId)
    },
  })
}

export function useMatchEventPresets(tournamentId: number, matchId: number, enabled = true) {
  return useQuery({
    queryKey: ['admin', 'tournaments', tournamentId, 'matches', matchId, 'event-presets'],
    queryFn: () => fetchMatchEventPresets(tournamentId, matchId),
    enabled: tournamentId > 0 && matchId > 0 && enabled,
  })
}

export function useAvailableCasters() {
  return useQuery({
    queryKey: ['admin', 'casters'],
    queryFn: fetchAvailableCasters,
  })
}

export function useMatchCasters(tournamentId: number, matchId: number, enabled = true) {
  return useQuery({
    queryKey: ['admin', 'tournaments', tournamentId, 'matches', matchId, 'casters'],
    queryFn: () => fetchMatchCasters(tournamentId, matchId),
    enabled: tournamentId > 0 && matchId > 0 && enabled,
  })
}

export function useAssignMatchCaster() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: ({ tournamentId, matchId, discordId }: { tournamentId: number; matchId: number; discordId: string }) =>
      assignMatchCaster(tournamentId, matchId, discordId),
    onSuccess: (_data, vars) => {
      qc.invalidateQueries({ queryKey: ['admin', 'tournaments', vars.tournamentId, 'matches', vars.matchId, 'casters'] })
    },
  })
}

export function useRemoveMatchCaster() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: ({ tournamentId, matchId, discordId }: { tournamentId: number; matchId: number; discordId: string }) =>
      removeMatchCaster(tournamentId, matchId, discordId),
    onSuccess: (_data, vars) => {
      qc.invalidateQueries({ queryKey: ['admin', 'tournaments', vars.tournamentId, 'matches', vars.matchId, 'casters'] })
    },
  })
}

export function useApplyMatchConvars() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: ({
      tournamentId,
      matchId,
      convars,
    }: {
      tournamentId: number
      matchId: number
      convars: Record<string, string | number | boolean>
    }) => applyMatchConvars(tournamentId, matchId, { convars }),
    onSuccess: (_data, vars) => {
      invalidateTournamentCaches(qc, vars.tournamentId)
      qc.invalidateQueries({
        queryKey: ['admin', 'tournaments', vars.tournamentId, 'matches', vars.matchId, 'event-presets'],
      })
    },
  })
}

export function useApplyMatchEventPreset() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: ({
      tournamentId,
      matchId,
      presetKey,
      enabled,
    }: {
      tournamentId: number
      matchId: number
      presetKey: string
      enabled?: boolean
    }) => applyMatchEventPreset(tournamentId, matchId, { preset_key: presetKey, enabled }),
    onSuccess: (_data, vars) => {
      invalidateTournamentCaches(qc, vars.tournamentId)
      qc.invalidateQueries({
        queryKey: ['admin', 'tournaments', vars.tournamentId, 'matches', vars.matchId, 'event-presets'],
      })
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

export function useInviteBySignup(tournamentId: number) {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: ({ teamId, signupId }: { teamId: number; signupId: number }) =>
      inviteBySignup(tournamentId, teamId, signupId),
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

export function useSetRecruitingStatus(tournamentId: number) {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: ({ teamId, status }: { teamId: number; status: string }) =>
      setRecruitingStatus(tournamentId, teamId, status),
    onSuccess: () => {
      invalidateTournamentCaches(qc, tournamentId)
    },
  })
}

export function useMyInvitations(tournamentId: number, enabled = true) {
  return useQuery({
    queryKey: ['tournaments', tournamentId, 'invitations'],
    queryFn: () => fetchMyInvitations(tournamentId),
    enabled: tournamentId > 0 && enabled,
    retry: false,
  })
}

export function useAcceptInvitation(tournamentId: number) {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (inviteId: number) => acceptInvitation(tournamentId, inviteId),
    onSuccess: () => {
      invalidateTournamentCaches(qc, tournamentId)
    },
  })
}

export function useRejectInvitation(tournamentId: number) {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (inviteId: number) => rejectInvitation(tournamentId, inviteId),
    onSuccess: () => {
      invalidateTournamentCaches(qc, tournamentId)
    },
  })
}

export function useApplyToTeam(tournamentId: number) {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (teamId: number) => applyToTeam(tournamentId, teamId),
    onSuccess: () => {
      invalidateTournamentCaches(qc, tournamentId)
    },
  })
}

export function useTeamApplications(tournamentId: number, teamId: number, enabled = false) {
  return useQuery({
    queryKey: ['tournaments', tournamentId, 'teams', teamId, 'applications'],
    queryFn: () => fetchTeamApplications(tournamentId, teamId),
    enabled: enabled && tournamentId > 0 && teamId > 0,
  })
}

export function useAcceptApplication(tournamentId: number) {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: ({ teamId, appId }: { teamId: number; appId: number }) =>
      acceptApplication(tournamentId, teamId, appId),
    onSuccess: () => {
      invalidateTournamentCaches(qc, tournamentId)
    },
  })
}

export function useRejectApplication(tournamentId: number) {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: ({ teamId, appId }: { teamId: number; appId: number }) =>
      rejectApplication(tournamentId, teamId, appId),
    onSuccess: () => {
      invalidateTournamentCaches(qc, tournamentId)
    },
  })
}

export function useConsent() {
  return useQuery({
    queryKey: ['consent'],
    queryFn: fetchConsent,
    retry: false,
  })
}

export function useSetConsent() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: () => setConsent(2),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['consent'] })
    },
  })
}

export function useRevokeConsent() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: revokeConsent,
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['consent'] })
    },
  })
}

export function useMyProfile() {
  return useQuery({
    queryKey: ['profile', 'me'],
    queryFn: fetchMyProfile,
    retry: false,
  })
}

export function useUpdateMyProfile() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (data: UserProfileUpdate) => updateMyProfile(data),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['profile', 'me'] })
    },
  })
}

export function useUploadProfileAvatar() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (file: File) => uploadProfileAvatar(file),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['profile', 'me'] })
    },
  })
}

export function useLeaderboard() {
  return useQuery({
    queryKey: ['leaderboard'],
    queryFn: fetchLeaderboard,
  })
}

export function usePlayerProfile(discordName: string) {
  return useQuery({
    queryKey: ['players', discordName],
    queryFn: () => fetchPlayerProfile(discordName),
    enabled: Boolean(discordName),
  })
}

// Voice
export function useVoiceMoveTeams(tournamentId: number) {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (matchId: number) => voiceMoveTeams(tournamentId, matchId),
    onSuccess: () => invalidateTournamentCaches(qc, tournamentId),
  })
}

export function useVoiceMoveSammelpunkt(tournamentId: number) {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: () => voiceMoveSammelpunkt(tournamentId),
    onSuccess: () => invalidateTournamentCaches(qc, tournamentId),
  })
}

export function useVoiceMoveUser() {
  return useMutation({
    mutationFn: ({ discordId, channelId }: { discordId: string; channelId: string }) =>
      voiceMoveUser(discordId, channelId),
  })
}

// Draft
export function useDraftHeroes() {
  return useQuery({
    queryKey: ['draft', 'heroes'],
    queryFn: fetchDraftHeroes,
    staleTime: Infinity,
  })
}

export function useDraftSession(sessionId: number | null) {
  return useQuery({
    queryKey: ['draft', 'session', sessionId],
    queryFn: () => fetchDraftSession(sessionId!),
    enabled: sessionId !== null,
    refetchInterval: 3000,
  })
}

export function useStartDraft() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (matchId: number) => startDraft(matchId),
    onSuccess: (data) => {
      qc.setQueryData(['draft', 'session', data.id], data)
    },
  })
}

export function useSubmitDraftAction(sessionId: number) {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: ({ heroName, takenBy }: { heroName: string; takenBy: string }) =>
      submitDraftAction(sessionId, heroName, takenBy),
    onSuccess: (data) => {
      qc.setQueryData(['draft', 'session', sessionId], data)
    },
  })
}

// Series
export function useSubmitSeriesGameResult(tournamentId: number, matchId: number) {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: ({
      gameNumber,
      winnerTeam,
      durationS,
    }: {
      gameNumber: number
      winnerTeam: 1 | 2
      durationS?: number
    }) => submitSeriesGameResult(tournamentId, matchId, gameNumber, winnerTeam, durationS),
    onSuccess: () => invalidateTournamentCaches(qc, tournamentId),
  })
}
