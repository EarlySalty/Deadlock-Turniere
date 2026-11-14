export function isRegistrationWindowOpen(status: string): boolean {
  return status === 'registration' || status === 'checkin'
}
