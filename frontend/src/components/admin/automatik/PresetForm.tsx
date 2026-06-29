import { useState, type FormEvent } from 'react'
import Button from '@/components/ui/Button'
import Card from '@/components/ui/Card'
import {
  useCreatePreset,
  useUpdatePreset,
} from '@/hooks/useAutomatik'
import type {
  BracketFormat,
  Category,
  InviteMode,
  Preset,
  PresetConfig,
  TournamentGameMode,
  TournamentMode,
} from '@/types/tournament'
import { AUTOMATIK_COPY } from './copy'

interface PresetFormProps {
  preset?: Preset | null
  onDone: () => void
}

const fieldClass =
  'w-full rounded-lg border border-border bg-background px-3 py-2 text-sm text-foreground focus:outline-none focus:ring-2 focus:ring-primary/50'

const textareaClass =
  'w-full rounded-lg border border-border bg-background px-3 py-2 text-sm text-foreground focus:outline-none focus:ring-2 focus:ring-primary/50'

function optionalText(value: string): string | null {
  const trimmed = value.trim()
  return trimmed.length > 0 ? trimmed : null
}

function configFromPreset(preset?: Preset | null): PresetConfig {
  return {
    team_size: preset?.team_size ?? 6,
    bracket_format: preset?.bracket_format ?? 'single_elimination',
    series_format: preset?.series_format ?? 1,
    final_series_format: preset?.final_series_format ?? null,
    tournament_mode: preset?.tournament_mode ?? 'group_stage',
    tournament_game_mode: preset?.tournament_game_mode ?? 'standard',
    match_objective: preset?.match_objective ?? 'auto',
    invite_mode: preset?.invite_mode ?? 'always',
    reminder_offsets: preset?.reminder_offsets ?? '[1440,120,15]',
    start_reminder_offsets: preset?.start_reminder_offsets ?? '[1440,60]',
    rules: preset?.rules ?? null,
    description_template: preset?.description_template ?? null,
  }
}

export default function PresetForm({ preset, onDone }: PresetFormProps) {
  const initialConfig = configFromPreset(preset)
  const [name, setName] = useState(preset?.name ?? '')
  const [category, setCategory] = useState<Category>(preset?.category ?? 'fun')
  const [teamSize, setTeamSize] = useState(initialConfig.team_size)
  const [bracketFormat, setBracketFormat] =
    useState<BracketFormat>(initialConfig.bracket_format)
  const [seriesFormat, setSeriesFormat] = useState(String(initialConfig.series_format))
  const [finalSeriesFormat, setFinalSeriesFormat] =
    useState(initialConfig.final_series_format ? String(initialConfig.final_series_format) : '')
  const [tournamentMode, setTournamentMode] =
    useState<TournamentMode>(initialConfig.tournament_mode)
  const [gameMode, setGameMode] =
    useState<TournamentGameMode>(initialConfig.tournament_game_mode)
  const [matchObjective, setMatchObjective] = useState(initialConfig.match_objective)
  const [inviteMode, setInviteMode] = useState<InviteMode>(initialConfig.invite_mode)
  const [reminderOffsets, setReminderOffsets] =
    useState(initialConfig.reminder_offsets ?? '')
  const [startReminderOffsets, setStartReminderOffsets] =
    useState(initialConfig.start_reminder_offsets ?? '')
  const [rules, setRules] = useState(initialConfig.rules ?? '')
  const [descriptionTemplate, setDescriptionTemplate] =
    useState(initialConfig.description_template ?? '')
  const [active, setActive] = useState(preset?.active ?? true)

  const createPreset = useCreatePreset()
  const updatePreset = useUpdatePreset()
  const isPending = createPreset.isPending || updatePreset.isPending

  const buildConfig = (): PresetConfig => ({
    team_size: teamSize,
    bracket_format: bracketFormat,
    series_format: Number(seriesFormat),
    final_series_format: finalSeriesFormat ? Number(finalSeriesFormat) : null,
    tournament_mode: tournamentMode,
    tournament_game_mode: gameMode,
    match_objective: matchObjective.trim() || 'auto',
    invite_mode: inviteMode,
    reminder_offsets: optionalText(reminderOffsets),
    start_reminder_offsets: optionalText(startReminderOffsets),
    rules: optionalText(rules),
    description_template: optionalText(descriptionTemplate),
  })

  const handleSubmit = (event: FormEvent) => {
    event.preventDefault()
    const payload = {
      name: name.trim(),
      category,
      config: buildConfig(),
    }
    if (preset) {
      updatePreset.mutate(
        { id: preset.id, data: payload },
        { onSuccess: onDone },
      )
      return
    }
    createPreset.mutate(
      { ...payload, active },
      { onSuccess: onDone },
    )
  }

  return (
    <Card className="p-5">
      <form onSubmit={handleSubmit} className="space-y-4">
        <div className="flex items-center justify-between gap-3">
          <h3 className="text-base font-semibold text-foreground">
            {preset
              ? AUTOMATIK_COPY.presetFormEditHeading
              : AUTOMATIK_COPY.presetFormCreateHeading}
          </h3>
          <Button type="button" variant="ghost" size="sm" onClick={onDone}>
            {AUTOMATIK_COPY.presetFormCancel}
          </Button>
        </div>

        <div className="grid gap-4 md:grid-cols-2">
          <label className="space-y-1.5 text-sm text-foreground">
            <span>{AUTOMATIK_COPY.nameLabel}</span>
            <input
              required
              value={name}
              onChange={(event) => setName(event.target.value)}
              className={fieldClass}
            />
          </label>

          <label className="space-y-1.5 text-sm text-foreground">
            <span>{AUTOMATIK_COPY.categoryLabel}</span>
            <select
              value={category}
              onChange={(event) => setCategory(event.target.value as Category)}
              className={fieldClass}
            >
              <option value="fun">{AUTOMATIK_COPY.categoryFun}</option>
              <option value="comp">{AUTOMATIK_COPY.categoryComp}</option>
            </select>
          </label>

          <label className="space-y-1.5 text-sm text-foreground">
            <span>{AUTOMATIK_COPY.teamSizeLabel}</span>
            <input
              type="number"
              min={1}
              value={teamSize}
              onChange={(event) => setTeamSize(Number(event.target.value))}
              className={fieldClass}
            />
          </label>

          <label className="space-y-1.5 text-sm text-foreground">
            <span>{AUTOMATIK_COPY.bracketFormatLabel}</span>
            <select
              value={bracketFormat}
              onChange={(event) =>
                setBracketFormat(event.target.value as BracketFormat)
              }
              className={fieldClass}
            >
              <option value="single_elimination">{AUTOMATIK_COPY.bracketSingle}</option>
              <option value="double_elimination">{AUTOMATIK_COPY.bracketDouble}</option>
            </select>
          </label>

          <label className="space-y-1.5 text-sm text-foreground">
            <span>{AUTOMATIK_COPY.seriesFormatLabel}</span>
            <select
              value={seriesFormat}
              onChange={(event) => setSeriesFormat(event.target.value)}
              className={fieldClass}
            >
              <option value="1">{AUTOMATIK_COPY.seriesBo1}</option>
              <option value="3">{AUTOMATIK_COPY.seriesBo3}</option>
              <option value="5">{AUTOMATIK_COPY.seriesBo5}</option>
            </select>
          </label>

          <label className="space-y-1.5 text-sm text-foreground">
            <span>{AUTOMATIK_COPY.finalSeriesFormatLabel}</span>
            <select
              value={finalSeriesFormat}
              onChange={(event) => setFinalSeriesFormat(event.target.value)}
              className={fieldClass}
            >
              <option value="">{AUTOMATIK_COPY.finalSeriesSame}</option>
              <option value="1">{AUTOMATIK_COPY.seriesBo1}</option>
              <option value="3">{AUTOMATIK_COPY.seriesBo3}</option>
              <option value="5">{AUTOMATIK_COPY.seriesBo5}</option>
            </select>
          </label>

          <label className="space-y-1.5 text-sm text-foreground">
            <span>{AUTOMATIK_COPY.tournamentModeLabel}</span>
            <select
              value={tournamentMode}
              onChange={(event) =>
                setTournamentMode(event.target.value as TournamentMode)
              }
              className={fieldClass}
            >
              <option value="group_stage">{AUTOMATIK_COPY.tournamentModeGroupStage}</option>
              <option value="bracket_only">{AUTOMATIK_COPY.tournamentModeBracketOnly}</option>
            </select>
          </label>

          <label className="space-y-1.5 text-sm text-foreground">
            <span>{AUTOMATIK_COPY.gameModeLabel}</span>
            <select
              value={gameMode}
              onChange={(event) =>
                setGameMode(event.target.value as TournamentGameMode)
              }
              className={fieldClass}
            >
              <option value="standard">{AUTOMATIK_COPY.gameModeStandard}</option>
              <option value="mirror">{AUTOMATIK_COPY.gameModeMirror}</option>
              <option value="all_same">{AUTOMATIK_COPY.gameModeAllSame}</option>
              <option value="random_heroes">{AUTOMATIK_COPY.gameModeRandomHeroes}</option>
              <option value="single_lane">{AUTOMATIK_COPY.gameModeSingleLane}</option>
            </select>
          </label>

          <label className="space-y-1.5 text-sm text-foreground">
            <span>{AUTOMATIK_COPY.matchObjectiveLabel}</span>
            <input
              value={matchObjective}
              onChange={(event) => setMatchObjective(event.target.value)}
              className={fieldClass}
            />
          </label>

          <label className="space-y-1.5 text-sm text-foreground">
            <span>{AUTOMATIK_COPY.inviteModeLabel}</span>
            <select
              value={inviteMode}
              onChange={(event) => setInviteMode(event.target.value as InviteMode)}
              className={fieldClass}
            >
              <option value="always">{AUTOMATIK_COPY.inviteAlways}</option>
              <option value="window">{AUTOMATIK_COPY.inviteWindow}</option>
              <option value="never">{AUTOMATIK_COPY.inviteNever}</option>
            </select>
          </label>

          <label className="space-y-1.5 text-sm text-foreground">
            <span>{AUTOMATIK_COPY.reminderOffsetsLabel}</span>
            <input
              value={reminderOffsets}
              onChange={(event) => setReminderOffsets(event.target.value)}
              className={fieldClass}
            />
          </label>

          <label className="space-y-1.5 text-sm text-foreground">
            <span>{AUTOMATIK_COPY.startReminderOffsetsLabel}</span>
            <input
              value={startReminderOffsets}
              onChange={(event) => setStartReminderOffsets(event.target.value)}
              className={fieldClass}
            />
          </label>
        </div>

        <label className="block space-y-1.5 text-sm text-foreground">
          <span>{AUTOMATIK_COPY.rulesLabel}</span>
          <textarea
            value={rules}
            onChange={(event) => setRules(event.target.value)}
            rows={4}
            className={textareaClass}
          />
        </label>

        <label className="block space-y-1.5 text-sm text-foreground">
          <span>{AUTOMATIK_COPY.descriptionTemplateLabel}</span>
          <textarea
            value={descriptionTemplate}
            onChange={(event) => setDescriptionTemplate(event.target.value)}
            rows={4}
            className={textareaClass}
          />
        </label>

        {!preset && (
          <label className="flex items-center gap-3 text-sm text-foreground">
            <input
              type="checkbox"
              checked={active}
              onChange={(event) => setActive(event.target.checked)}
              className="h-4 w-4 accent-primary"
            />
            <span>{AUTOMATIK_COPY.activeCheckboxLabel}</span>
          </label>
        )}

        <Button type="submit" disabled={isPending}>
          {AUTOMATIK_COPY.presetFormSave}
        </Button>
      </form>
    </Card>
  )
}
