export const REGISTRATION_WINDOW_ERROR = 'Der Turnier-Start (Anmeldeschluss) darf nicht vor dem Anmeldestart liegen.'

export function registrationWindowError(startIso: string | undefined, endIso: string | undefined): string {
  if (!startIso || !endIso) return ''
  return Date.parse(endIso) < Date.parse(startIso) ? REGISTRATION_WINDOW_ERROR : ''
}
