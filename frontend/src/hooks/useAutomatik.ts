import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import {
  applyProposalEvent,
  clearMyDmOptout,
  createManualProposal,
  createPreset,
  deletePreset,
  fetchMyDmOptout,
  fetchPreset,
  fetchProposal,
  fetchProposals,
  fetchPresets,
  recordProposalFeedback,
  recordProposalVote,
  setMyDmOptout,
  setPresetActive,
  updatePreset,
} from '@/api/client'
import type {
  DmScope,
  NewPresetInput,
  PresetUpdateInput,
  ProposalEventInput,
  ProposalState,
  VoteDecision,
} from '@/types/tournament'

export function usePresets() {
  return useQuery({
    queryKey: ['admin', 'presets'],
    queryFn: fetchPresets,
  })
}

export function usePreset(id: number) {
  return useQuery({
    queryKey: ['admin', 'presets', id],
    queryFn: () => fetchPreset(id),
    enabled: id > 0,
  })
}

export function useCreatePreset() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (data: NewPresetInput) => createPreset(data),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['admin', 'presets'] })
    },
  })
}

export function useUpdatePreset() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: ({ id, data }: { id: number; data: PresetUpdateInput }) =>
      updatePreset(id, data),
    onSuccess: (_data, vars) => {
      qc.invalidateQueries({ queryKey: ['admin', 'presets'] })
      qc.invalidateQueries({ queryKey: ['admin', 'presets', vars.id] })
    },
  })
}

export function useSetPresetActive() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: ({ id, active }: { id: number; active: boolean }) =>
      setPresetActive(id, active),
    onSuccess: (_data, vars) => {
      qc.invalidateQueries({ queryKey: ['admin', 'presets'] })
      qc.invalidateQueries({ queryKey: ['admin', 'presets', vars.id] })
    },
  })
}

export function useDeletePreset() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (id: number) => deletePreset(id),
    onSuccess: (_data, id) => {
      qc.invalidateQueries({ queryKey: ['admin', 'presets'] })
      qc.removeQueries({ queryKey: ['admin', 'presets', id] })
    },
  })
}

export function useProposals(state?: ProposalState) {
  return useQuery({
    queryKey: ['admin', 'proposals', state ?? 'all'],
    queryFn: () => fetchProposals(state),
  })
}

export function useProposal(id: number | null) {
  return useQuery({
    queryKey: ['admin', 'proposals', id],
    queryFn: () => fetchProposal(id ?? 0),
    enabled: Boolean(id),
  })
}

export function useCreateManualProposal() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (data: {
      preset_id: number
      name: string
      proposed_start?: string | null
    }) => createManualProposal(data),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['admin', 'proposals'] })
    },
  })
}

export function useApplyProposalEvent() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: ({ id, event }: { id: number; event: ProposalEventInput }) =>
      applyProposalEvent(id, event),
    onSuccess: (_data, vars) => {
      qc.invalidateQueries({ queryKey: ['admin', 'proposals'] })
      qc.invalidateQueries({ queryKey: ['admin', 'proposals', vars.id] })
    },
  })
}

export function useRecordProposalVote() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: ({
      id,
      caster_id,
      decision,
    }: {
      id: number
      caster_id: string
      decision: VoteDecision
    }) => recordProposalVote(id, { caster_id, decision }),
    onSuccess: (_data, vars) => {
      qc.invalidateQueries({ queryKey: ['admin', 'proposals'] })
      qc.invalidateQueries({ queryKey: ['admin', 'proposals', vars.id] })
    },
  })
}

export function useRecordProposalFeedback() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: ({ id, raw_text }: { id: number; raw_text: string }) =>
      recordProposalFeedback(id, { raw_text }),
    onSuccess: (_data, vars) => {
      qc.invalidateQueries({ queryKey: ['admin', 'proposals'] })
      qc.invalidateQueries({ queryKey: ['admin', 'proposals', vars.id] })
    },
  })
}

export function useMyDmOptout() {
  return useQuery({
    queryKey: ['me', 'dm-optout'],
    queryFn: fetchMyDmOptout,
  })
}

export function useSetMyDmOptout() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (scope: DmScope) => setMyDmOptout(scope),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['me', 'dm-optout'] })
    },
  })
}

export function useClearMyDmOptout() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (scope: DmScope) => clearMyDmOptout(scope),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['me', 'dm-optout'] })
    },
  })
}
