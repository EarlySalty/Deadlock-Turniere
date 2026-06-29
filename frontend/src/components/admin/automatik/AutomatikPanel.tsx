import { useMemo, useState, type FormEvent } from 'react'
import {
  CalendarClock,
  CheckCircle2,
  Edit2,
  MessageSquare,
  Plus,
  Power,
  Trash2,
  Vote,
  XCircle,
} from 'lucide-react'
import Button from '@/components/ui/Button'
import Card from '@/components/ui/Card'
import DateTimeInput from '@/components/ui/DateTimeInput'
import LoadingSpinner from '@/components/ui/LoadingSpinner'
import {
  useApplyProposalEvent,
  useCreateManualProposal,
  useDeletePreset,
  usePreset,
  usePresets,
  useProposal,
  useProposals,
  useRecordProposalFeedback,
  useRecordProposalVote,
  useSetPresetActive,
} from '@/hooks/useAutomatik'
import type {
  Preset,
  Proposal,
  ProposalEventInput,
  ProposalSource,
  ProposalState,
  VoteDecision,
} from '@/types/tournament'
import PresetForm from './PresetForm'
import { AUTOMATIK_COPY } from './copy'

const inputClass =
  'w-full rounded-lg border border-border bg-background px-3 py-2 text-sm text-foreground focus:outline-none focus:ring-2 focus:ring-primary/50'

const textareaClass =
  'w-full rounded-lg border border-border bg-background px-3 py-2 text-sm text-foreground focus:outline-none focus:ring-2 focus:ring-primary/50'

const stateLabels: Record<ProposalState, string> = {
  draft: AUTOMATIK_COPY.stateDraft,
  pending_approval: AUTOMATIK_COPY.statePendingApproval,
  approved: AUTOMATIK_COPY.stateApproved,
  rejected: AUTOMATIK_COPY.stateRejected,
  expired: AUTOMATIK_COPY.stateExpired,
}

const sourceLabels: Record<ProposalSource, string> = {
  bot: AUTOMATIK_COPY.sourceBot,
  manual: AUTOMATIK_COPY.sourceManual,
}

const proposalEvents: { event: ProposalEventInput; label: string }[] = [
  { event: 'submit', label: AUTOMATIK_COPY.submitEvent },
  { event: 'approve', label: AUTOMATIK_COPY.approveEvent },
  { event: 'reject', label: AUTOMATIK_COPY.rejectEvent },
  { event: 'expire', label: AUTOMATIK_COPY.expireEvent },
]

function categoryLabel(category: Preset['category']) {
  return category === 'fun' ? AUTOMATIK_COPY.categoryFun : AUTOMATIK_COPY.categoryComp
}

function presetSummary(preset: Preset) {
  return `${preset.team_size} | ${preset.bracket_format} | ${preset.series_format}`
}

export default function AutomatikPanel() {
  const { data: presets = [], isLoading: presetsLoading } = usePresets()
  const { data: proposals = [], isLoading: proposalsLoading } = useProposals()
  const [showPresetForm, setShowPresetForm] = useState(false)
  const [editingPreset, setEditingPreset] = useState<Preset | null>(null)
  const [selectedProposalId, setSelectedProposalId] = useState<number | null>(null)
  const [manualPresetId, setManualPresetId] = useState<number | ''>('')
  const [manualName, setManualName] = useState('')
  const [manualStart, setManualStart] = useState('')
  const [feedbackText, setFeedbackText] = useState('')
  const [voteCasterId, setVoteCasterId] = useState('')
  const [voteDecision, setVoteDecision] = useState<VoteDecision>('approve')

  const selectedPresetQuery = usePreset(editingPreset?.id ?? 0)
  const { data: proposalDetail } = useProposal(selectedProposalId)
  const setPresetActive = useSetPresetActive()
  const deletePreset = useDeletePreset()
  const createManualProposal = useCreateManualProposal()
  const applyProposalEvent = useApplyProposalEvent()
  const recordFeedback = useRecordProposalFeedback()
  const recordVote = useRecordProposalVote()

  const presetById = useMemo(
    () => new Map(presets.map((preset) => [preset.id, preset])),
    [presets],
  )

  const activePreset = editingPreset
    ? selectedPresetQuery.data ?? editingPreset
    : null

  const handleDeletePreset = (preset: Preset) => {
    if (!window.confirm(AUTOMATIK_COPY.presetDeleteConfirm)) return
    deletePreset.mutate(preset.id)
    if (editingPreset?.id === preset.id) {
      setEditingPreset(null)
      setShowPresetForm(false)
    }
  }

  const handleManualSubmit = (event: FormEvent) => {
    event.preventDefault()
    if (!manualPresetId) return
    createManualProposal.mutate(
      {
        preset_id: manualPresetId,
        name: manualName.trim(),
        proposed_start: manualStart || null,
      },
      {
        onSuccess: (proposal) => {
          setSelectedProposalId(proposal.id)
          setManualName('')
          setManualStart('')
        },
      },
    )
  }

  const handleFeedbackSubmit = (event: FormEvent) => {
    event.preventDefault()
    if (!selectedProposalId || !feedbackText.trim()) return
    recordFeedback.mutate(
      { id: selectedProposalId, raw_text: feedbackText.trim() },
      { onSuccess: () => setFeedbackText('') },
    )
  }

  const handleVoteSubmit = (event: FormEvent) => {
    event.preventDefault()
    if (!selectedProposalId || !voteCasterId.trim()) return
    recordVote.mutate({
      id: selectedProposalId,
      caster_id: voteCasterId.trim(),
      decision: voteDecision,
    })
  }

  return (
    <div className="space-y-6">
      <header className="flex flex-col gap-2">
        <h2 className="flex items-center gap-2 text-xl font-semibold text-foreground">
          <CalendarClock size={20} className="text-primary" />
          {AUTOMATIK_COPY.panelHeading}
        </h2>
        <p className="text-sm text-muted">{AUTOMATIK_COPY.panelDescription}</p>
      </header>

      <section className="grid gap-6 xl:grid-cols-[minmax(0,1fr)_minmax(360px,0.8fr)]">
        <Card className="p-5">
          <div className="mb-4 flex items-center justify-between gap-3">
            <h3 className="text-lg font-semibold text-foreground">
              {AUTOMATIK_COPY.presetsHeading}
            </h3>
            <Button
              type="button"
              size="sm"
              onClick={() => {
                setEditingPreset(null)
                setShowPresetForm(true)
              }}
            >
              <Plus size={14} />
              {AUTOMATIK_COPY.presetCreateButton}
            </Button>
          </div>

          {presetsLoading ? (
            <LoadingSpinner />
          ) : presets.length === 0 ? (
            <p className="rounded-lg border border-dashed border-border p-4 text-center text-sm text-muted">
              {AUTOMATIK_COPY.presetsEmpty}
            </p>
          ) : (
            <div className="grid gap-3 md:grid-cols-2">
              {presets.map((preset) => (
                <article
                  key={preset.id}
                  className="rounded-lg border border-border bg-white/[0.02] p-4"
                >
                  <div className="flex items-start justify-between gap-3">
                    <div className="min-w-0">
                      <h4 className="truncate text-sm font-semibold text-foreground">
                        {preset.name}
                      </h4>
                      <div className="mt-2 flex flex-wrap gap-2 text-[11px]">
                        <span className="rounded-full bg-primary/15 px-2 py-0.5 text-primary">
                          {categoryLabel(preset.category)}
                        </span>
                        <span
                          className={`rounded-full px-2 py-0.5 ${
                            preset.active
                              ? 'bg-green-500/15 text-green-300'
                              : 'bg-white/10 text-muted'
                          }`}
                        >
                          {preset.active
                            ? AUTOMATIK_COPY.presetActive
                            : AUTOMATIK_COPY.presetInactive}
                        </span>
                      </div>
                    </div>
                    <button
                      type="button"
                      onClick={() =>
                        setPresetActive.mutate({
                          id: preset.id,
                          active: !preset.active,
                        })
                      }
                      className="rounded p-1 text-muted hover:bg-white/5 hover:text-foreground"
                    >
                      {preset.active ? <XCircle size={16} /> : <Power size={16} />}
                    </button>
                  </div>

                  <p className="mt-3 text-xs text-muted">
                    {AUTOMATIK_COPY.presetInfoLabel}: {presetSummary(preset)}
                  </p>

                  <div className="mt-4 flex flex-wrap gap-2">
                    <Button
                      type="button"
                      size="sm"
                      variant="secondary"
                      onClick={() => {
                        setEditingPreset(preset)
                        setShowPresetForm(true)
                      }}
                    >
                      <Edit2 size={13} />
                      {AUTOMATIK_COPY.presetEditButton}
                    </Button>
                    <Button
                      type="button"
                      size="sm"
                      variant="danger"
                      onClick={() => handleDeletePreset(preset)}
                      disabled={deletePreset.isPending}
                    >
                      <Trash2 size={13} />
                      {AUTOMATIK_COPY.presetDeleteButton}
                    </Button>
                  </div>
                </article>
              ))}
            </div>
          )}
        </Card>

        {showPresetForm && (
          <PresetForm
            preset={activePreset}
            onDone={() => {
              setShowPresetForm(false)
              setEditingPreset(null)
            }}
          />
        )}
      </section>

      <section className="grid gap-6 xl:grid-cols-[360px_minmax(0,1fr)]">
        <Card className="p-5">
          <h3 className="mb-4 text-lg font-semibold text-foreground">
            {AUTOMATIK_COPY.manualHeading}
          </h3>
          <form onSubmit={handleManualSubmit} className="space-y-4">
            <label className="block space-y-1.5 text-sm text-foreground">
              <span>{AUTOMATIK_COPY.presetSelectLabel}</span>
              <select
                value={manualPresetId}
                onChange={(event) =>
                  setManualPresetId(
                    event.target.value ? Number(event.target.value) : '',
                  )
                }
                className={inputClass}
              >
                <option value="">{AUTOMATIK_COPY.noPresetOption}</option>
                {presets.map((preset) => (
                  <option key={preset.id} value={preset.id}>
                    {preset.name}
                  </option>
                ))}
              </select>
            </label>

            <label className="block space-y-1.5 text-sm text-foreground">
              <span>{AUTOMATIK_COPY.nameLabel}</span>
              <input
                required
                value={manualName}
                onChange={(event) => setManualName(event.target.value)}
                className={inputClass}
              />
            </label>

            <label className="block space-y-1.5 text-sm text-foreground">
              <span>{AUTOMATIK_COPY.proposedStartLabel}</span>
              <DateTimeInput
                value={manualStart}
                onChange={(event) => setManualStart(event.target.value)}
                className={inputClass}
              />
            </label>

            <Button
              type="submit"
              disabled={!manualPresetId || !manualName.trim() || createManualProposal.isPending}
            >
              {AUTOMATIK_COPY.manualCreateButton}
            </Button>
          </form>
        </Card>

        <Card className="p-5">
          <div className="mb-4 flex items-center gap-2">
            <Vote size={18} className="text-primary" />
            <h3 className="text-lg font-semibold text-foreground">
              {AUTOMATIK_COPY.proposalsHeading}
            </h3>
          </div>

          {proposalsLoading ? (
            <LoadingSpinner />
          ) : proposals.length === 0 ? (
            <p className="rounded-lg border border-dashed border-border p-4 text-center text-sm text-muted">
              {AUTOMATIK_COPY.proposalsEmpty}
            </p>
          ) : (
            <div className="grid gap-3 md:grid-cols-2">
              {proposals.map((proposal: Proposal) => {
                const preset = proposal.preset_id
                  ? presetById.get(proposal.preset_id)
                  : undefined
                const selected = selectedProposalId === proposal.id
                return (
                  <button
                    key={proposal.id}
                    type="button"
                    onClick={() => setSelectedProposalId(proposal.id)}
                    className={`rounded-lg border p-4 text-left transition-colors ${
                      selected
                        ? 'border-primary/60 bg-primary/10'
                        : 'border-border bg-white/[0.02] hover:bg-white/[0.05]'
                    }`}
                  >
                    <div className="flex items-start justify-between gap-3">
                      <div className="min-w-0">
                        <p className="truncate text-sm font-semibold text-foreground">
                          {preset?.name ?? proposal.config_json}
                        </p>
                        <p className="mt-1 text-xs text-muted">
                          {AUTOMATIK_COPY.proposalPresetLabel}: {proposal.preset_id ?? '-'}
                        </p>
                      </div>
                      <span className="rounded-full bg-white/10 px-2 py-0.5 text-[11px] text-muted">
                        {stateLabels[proposal.state]}
                      </span>
                    </div>
                    <div className="mt-3 flex flex-wrap gap-2 text-[11px] text-muted">
                      <span>{sourceLabels[proposal.source]}</span>
                      <span>{new Date(proposal.created_at).toLocaleString('de-DE')}</span>
                    </div>
                  </button>
                )
              })}
            </div>
          )}
        </Card>
      </section>

      <Card className="p-5">
        {!selectedProposalId ? (
          <p className="text-sm text-muted">{AUTOMATIK_COPY.proposalSelectHint}</p>
        ) : !proposalDetail ? (
          <LoadingSpinner />
        ) : (
          <div className="space-y-5">
            <div className="flex flex-col gap-3 md:flex-row md:items-start md:justify-between">
              <div>
                <h3 className="flex items-center gap-2 text-lg font-semibold text-foreground">
                  <MessageSquare size={18} className="text-primary" />
                  {AUTOMATIK_COPY.proposalDetailHeading}
                </h3>
                <dl className="mt-3 grid gap-2 text-sm text-muted md:grid-cols-2">
                  <div>
                    <dt className="text-xs uppercase text-muted">
                      {AUTOMATIK_COPY.proposalStateLabel}
                    </dt>
                    <dd className="text-foreground">
                      {stateLabels[proposalDetail.proposal.state]}
                    </dd>
                  </div>
                  <div>
                    <dt className="text-xs uppercase text-muted">
                      {AUTOMATIK_COPY.proposalSourceLabel}
                    </dt>
                    <dd className="text-foreground">
                      {sourceLabels[proposalDetail.proposal.source]}
                    </dd>
                  </div>
                  <div>
                    <dt className="text-xs uppercase text-muted">
                      {AUTOMATIK_COPY.proposalCreatedLabel}
                    </dt>
                    <dd className="text-foreground">
                      {new Date(proposalDetail.proposal.created_at).toLocaleString('de-DE')}
                    </dd>
                  </div>
                  <div>
                    <dt className="text-xs uppercase text-muted">
                      {AUTOMATIK_COPY.proposalApprovalsLabel}
                    </dt>
                    <dd className="text-foreground">{proposalDetail.approvals}</dd>
                  </div>
                </dl>
              </div>

              <div className="flex flex-wrap gap-2">
                {proposalEvents.map((item) => (
                  <Button
                    key={item.event}
                    type="button"
                    size="sm"
                    variant={item.event === 'reject' ? 'danger' : 'secondary'}
                    disabled={applyProposalEvent.isPending}
                    onClick={() =>
                      applyProposalEvent.mutate({
                        id: proposalDetail.proposal.id,
                        event: item.event,
                      })
                    }
                  >
                    {item.event === 'approve' ? <CheckCircle2 size={13} /> : null}
                    {item.label}
                  </Button>
                ))}
              </div>
            </div>

            <details className="rounded-lg border border-border">
              <summary className="cursor-pointer px-4 py-3 text-sm font-medium text-foreground">
                {AUTOMATIK_COPY.proposalConfigLabel}
              </summary>
              <pre className="max-h-64 overflow-auto border-t border-border p-4 text-xs text-muted">
                {proposalDetail.proposal.config_json}
              </pre>
            </details>

            <div className="grid gap-5 lg:grid-cols-2">
              <section className="space-y-3">
                <h4 className="text-sm font-semibold text-foreground">
                  {AUTOMATIK_COPY.votesHeading}
                </h4>
                <div className="space-y-2">
                  {proposalDetail.votes.map((vote) => (
                    <div
                      key={vote.id}
                      className="flex items-center justify-between gap-3 rounded-lg border border-border px-3 py-2 text-sm"
                    >
                      <span className="truncate text-foreground">
                        {vote.caster_discord_id}
                      </span>
                      <span className="text-muted">
                        {vote.decision === 'approve'
                          ? AUTOMATIK_COPY.voteApprove
                          : AUTOMATIK_COPY.voteReject}
                      </span>
                    </div>
                  ))}
                </div>

                <form onSubmit={handleVoteSubmit} className="grid gap-2 sm:grid-cols-[1fr_140px_auto]">
                  <input
                    value={voteCasterId}
                    onChange={(event) => setVoteCasterId(event.target.value)}
                    placeholder={AUTOMATIK_COPY.voteCasterLabel}
                    className={inputClass}
                  />
                  <select
                    value={voteDecision}
                    onChange={(event) =>
                      setVoteDecision(event.target.value as VoteDecision)
                    }
                    className={inputClass}
                  >
                    <option value="approve">{AUTOMATIK_COPY.voteApprove}</option>
                    <option value="reject">{AUTOMATIK_COPY.voteReject}</option>
                  </select>
                  <Button type="submit" size="sm" disabled={recordVote.isPending}>
                    {AUTOMATIK_COPY.voteSubmit}
                  </Button>
                </form>
              </section>

              <section className="space-y-3">
                <h4 className="text-sm font-semibold text-foreground">
                  {AUTOMATIK_COPY.feedbackHeading}
                </h4>
                <div className="space-y-2">
                  {proposalDetail.feedback.map((entry) => (
                    <div
                      key={entry.id}
                      className="rounded-lg border border-border px-3 py-2 text-sm"
                    >
                      <p className="text-xs text-muted">{entry.caster_discord_id}</p>
                      <p className="mt-1 whitespace-pre-wrap text-foreground">
                        {entry.raw_text}
                      </p>
                    </div>
                  ))}
                </div>

                <form onSubmit={handleFeedbackSubmit} className="space-y-2">
                  <textarea
                    value={feedbackText}
                    onChange={(event) => setFeedbackText(event.target.value)}
                    placeholder={AUTOMATIK_COPY.feedbackPlaceholder}
                    rows={4}
                    className={textareaClass}
                  />
                  <Button
                    type="submit"
                    size="sm"
                    disabled={!feedbackText.trim() || recordFeedback.isPending}
                  >
                    {AUTOMATIK_COPY.feedbackSubmit}
                  </Button>
                </form>
              </section>
            </div>
          </div>
        )}
      </Card>
    </div>
  )
}
