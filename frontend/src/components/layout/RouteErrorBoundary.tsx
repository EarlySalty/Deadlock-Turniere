import { Component, type ReactNode } from 'react'

type Props = { children: ReactNode }
type State = { failed: boolean }

export default class RouteErrorBoundary extends Component<Props, State> {
  state: State = { failed: false }

  static getDerivedStateFromError(): State {
    return { failed: true }
  }

  render() {
    if (!this.state.failed) return this.props.children
    return (
      <div role="alert" className="mx-auto max-w-7xl px-6 py-12 text-sm text-muted">
        <p>Diese Ansicht konnte nicht geladen werden.</p>
        <button
          type="button"
          onClick={() => window.location.reload()}
          className="mt-4 rounded-lg border border-border px-4 py-2 text-foreground hover:bg-white/5"
        >
          Seite neu laden
        </button>
      </div>
    )
  }
}
