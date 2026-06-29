import { Bell } from 'lucide-react'
import Card from '@/components/ui/Card'
import LoadingSpinner from '@/components/ui/LoadingSpinner'
import {
  useClearMyDmOptout,
  useMyDmOptout,
  useSetMyDmOptout,
} from '@/hooks/useAutomatik'
import type { DmScope } from '@/types/tournament'
import { ACCOUNT_COPY } from './copy'

const scopes: { scope: DmScope; label: string }[] = [
  { scope: 'fun', label: ACCOUNT_COPY.scopeFun },
  { scope: 'comp', label: ACCOUNT_COPY.scopeComp },
  { scope: 'all', label: ACCOUNT_COPY.scopeAll },
]

export default function DmSubscriptionPanel() {
  const { data, isLoading } = useMyDmOptout()
  const setOptout = useSetMyDmOptout()
  const clearOptout = useClearMyDmOptout()
  const activeScopes = data?.scopes ?? []
  const isPending = setOptout.isPending || clearOptout.isPending

  const handleToggle = (scope: DmScope, subscribed: boolean) => {
    if (subscribed) {
      clearOptout.mutate(scope)
      return
    }
    setOptout.mutate(scope)
  }

  return (
    <Card className="p-6 space-y-5 border-white/5">
      <header className="flex items-start gap-3">
        <Bell size={18} className="mt-0.5 text-primary" />
        <div>
          <h3 className="text-sm font-bold uppercase tracking-wide text-foreground">
            {ACCOUNT_COPY.dmPanelHeading}
          </h3>
          <p className="mt-1 text-xs text-muted">
            {ACCOUNT_COPY.dmPanelDescription}
          </p>
        </div>
      </header>

      {isLoading ? (
        <div className="py-4">
          <LoadingSpinner />
        </div>
      ) : (
        <div className="space-y-3">
          {scopes.map((item) => {
            const subscribed = !activeScopes.includes(item.scope)
            return (
              <label
                key={item.scope}
                className="flex items-center justify-between gap-4 rounded-lg border border-white/10 px-3 py-3"
              >
                <span>
                  <span className="block text-sm font-semibold text-foreground">
                    {item.label}
                  </span>
                  <span className="text-xs text-muted">
                    {subscribed ? ACCOUNT_COPY.subscribed : ACCOUNT_COPY.optedOut}
                  </span>
                </span>
                <input
                  type="checkbox"
                  checked={subscribed}
                  disabled={isPending}
                  onChange={(event) => handleToggle(item.scope, event.target.checked)}
                  className="h-4 w-4 accent-primary"
                />
              </label>
            )
          })}
        </div>
      )}
    </Card>
  )
}
