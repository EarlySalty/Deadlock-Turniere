export const LOBBY_CODE_NOT_DELIVERED =
  'Der Code ist nicht im Discord-Match-Channel angekommen. Sende ihn mit „Code an Teams senden“ erneut oder gib ihn den Teams selbst.'

export function lobbyDeliveryMessage(
  successText: string,
  discordChannelId: string | null | undefined,
): { kind: 'success' | 'error'; text: string } {
  if (discordChannelId) return { kind: 'success', text: `${successText} Der Code ist im Discord-Match-Channel.` }
  return { kind: 'error', text: `${successText} ${LOBBY_CODE_NOT_DELIVERED}` }
}

export function lobbyResendMessage(discordChannelId: string | null | undefined): {
  kind: 'success' | 'error'
  text: string
} {
  if (discordChannelId) return { kind: 'success', text: 'Der Code ist im Discord-Match-Channel.' }
  return { kind: 'error', text: LOBBY_CODE_NOT_DELIVERED }
}
