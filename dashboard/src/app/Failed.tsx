import { Component, type ReactNode } from 'react'

/** A page that failed to draw says so, instead of leaving the screen blank. */
export class Failed extends Component<{ children: ReactNode }, { failed: boolean }> {
  override state: { failed: boolean } = { failed: false }

  static getDerivedStateFromError(): { failed: boolean } {
    return { failed: true }
  }

  override render(): ReactNode {
    if (this.state.failed) {
      return (
        <p role="alert">
          This page could not be shown. Reload it. If it fails again, the factory's log has the
          cause.
        </p>
      )
    }
    return this.props.children
  }
}
