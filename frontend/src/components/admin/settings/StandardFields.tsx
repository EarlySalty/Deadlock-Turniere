import DateTimeInput from '@/components/ui/DateTimeInput'
import type { BracketFormat } from '@/types/tournament'
import Field from './Field'
import ReminderOffsetsInput from '../ReminderOffsetsInput'
import { SubHeading } from './SettingsSection'
import { INPUT_CLASS } from './options'
import type { SeriesFormat } from './createFormValues'

export interface BasicsValue {
  name: string
  description: string
  rules: string
}

export interface MatchFormatValue {
  teamSize: number
  bracketFormat: BracketFormat
  seriesFormat: SeriesFormat
  finalSeriesFormat: SeriesFormat | null
}

export interface ReminderValue {
  reminderOffsets: number[]
  startReminderOffsets: number[]
}

export interface DateEntry {
  key: string
  label: string
  hint?: string
  value: string
  onChange: (value: string) => void
}

interface FieldGroupProps<T> {
  idPrefix: string
  value: T
  onChange: (patch: Partial<T>) => void
}

export function BasicsFields({ idPrefix, value, onChange, required = false }: FieldGroupProps<BasicsValue> & { required?: boolean }) {
  return (
    <div className="space-y-4">
      <Field id={`${idPrefix}-name`} label={required ? 'Turniername *' : 'Turniername'}>
        <input
          id={`${idPrefix}-name`}
          type="text"
          required={required}
          value={value.name}
          onChange={(event) => onChange({ name: event.target.value })}
          placeholder="z. B. Deadlock Community Cup #1"
          className={INPUT_CLASS}
        />
      </Field>
      <div className="grid gap-4 lg:grid-cols-2">
        <Field id={`${idPrefix}-description`} label="Beschreibung" hint="Kurzer Text für die Turnierseite.">
          <textarea
            id={`${idPrefix}-description`}
            value={value.description}
            onChange={(event) => onChange({ description: event.target.value })}
            rows={6}
            placeholder="Worum geht es, wer kann mitmachen?"
            className={`${INPUT_CLASS} resize-y`}
          />
        </Field>
        <Field id={`${idPrefix}-rules`} label="Regelwerk" hint="Markdown wird unterstützt.">
          <textarea
            id={`${idPrefix}-rules`}
            value={value.rules}
            onChange={(event) => onChange({ rules: event.target.value })}
            rows={6}
            placeholder="Regeln für dieses Turnier..."
            className={`${INPUT_CLASS} resize-y`}
          />
        </Field>
      </div>
    </div>
  )
}

export function MatchFormatFields({ idPrefix, value, onChange, league }: FieldGroupProps<MatchFormatValue> & { league: boolean }) {
  return (
    <div className="grid gap-4 sm:grid-cols-2 xl:grid-cols-4">
      <Field id={`${idPrefix}-team-size`} label="Teamgröße" hint="Spieler pro Team.">
        <input
          id={`${idPrefix}-team-size`}
          type="number"
          min={1}
          max={20}
          value={value.teamSize}
          onChange={(event) => onChange({ teamSize: Number(event.target.value) })}
          className={INPUT_CLASS}
        />
      </Field>
      {!league && (
        <>
          <Field id={`${idPrefix}-bracket-format`} label="Bracket-Format" hint="Double: erst nach zwei Niederlagen raus.">
            <select
              id={`${idPrefix}-bracket-format`}
              value={value.bracketFormat}
              onChange={(event) => onChange({ bracketFormat: event.target.value as BracketFormat })}
              className={INPUT_CLASS}
            >
              <option value="single_elimination">Single Elimination</option>
              <option value="double_elimination">Double Elimination</option>
            </select>
          </Field>
          <Field id={`${idPrefix}-series-format`} label="Serienformat" hint="Spiele pro Begegnung.">
            <select
              id={`${idPrefix}-series-format`}
              value={value.seriesFormat}
              onChange={(event) => onChange({ seriesFormat: Number(event.target.value) as SeriesFormat })}
              className={INPUT_CLASS}
            >
              <option value={1}>Bo1</option>
              <option value={3}>Bo3</option>
              <option value={5}>Bo5</option>
            </select>
          </Field>
          <Field id={`${idPrefix}-final-series-format`} label="Finale-Format" hint="Nur für das Finale.">
            <select
              id={`${idPrefix}-final-series-format`}
              value={value.finalSeriesFormat ?? 'same'}
              onChange={(event) => {
                const next = event.target.value
                onChange({ finalSeriesFormat: next === 'same' ? null : (Number(next) as SeriesFormat) })
              }}
              className={INPUT_CLASS}
            >
              <option value="same">Wie Serienformat</option>
              <option value={1}>Bo1</option>
              <option value={3}>Bo3</option>
              <option value={5}>Bo5</option>
            </select>
          </Field>
        </>
      )}
    </div>
  )
}

export function DateFields({ idPrefix, entries }: { idPrefix: string; entries: DateEntry[] }) {
  return (
    <div className="space-y-4">
      <SubHeading>Termine</SubHeading>
      <div className="grid gap-4 sm:grid-cols-2 xl:grid-cols-3">
        {entries.map((entry) => (
          <Field key={entry.key} id={`${idPrefix}-${entry.key}`} label={entry.label} hint={entry.hint}>
            <DateTimeInput
              id={`${idPrefix}-${entry.key}`}
              value={entry.value}
              onChange={(event) => entry.onChange(event.target.value)}
              className={INPUT_CLASS}
            />
          </Field>
        ))}
      </div>
    </div>
  )
}

export function ReminderFields({ idPrefix, value, onChange }: FieldGroupProps<ReminderValue>) {
  return (
    <div className="space-y-4">
      <SubHeading>Erinnerungen</SubHeading>
      <div className="grid gap-4 sm:grid-cols-2">
        <ReminderOffsetsInput
          id={`${idPrefix}-reminder-offsets`}
          label="Reminder vor Turnier-Start"
          value={value.reminderOffsets}
          onChange={(reminderOffsets) => onChange({ reminderOffsets })}
          hint="Geht an alle mit Anmelde-Erinnerung, solange die Anmeldung offen ist. Ohne Auswahl gelten 1 Tag, 2 Std und 15 Min."
        />
        <ReminderOffsetsInput
          id={`${idPrefix}-start-reminder-offsets`}
          label="Erinnerung vor Match-Start"
          value={value.startReminderOffsets}
          onChange={(startReminderOffsets) => onChange({ startReminderOffsets })}
          hint="Geht an alle angemeldeten Teilnehmer. Ohne Auswahl gelten 1 Tag und 1 Std."
        />
      </div>
    </div>
  )
}
