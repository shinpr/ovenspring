import { useState } from 'react'
import { ActionPanel } from '../ActionPanel/ActionPanel.tsx'
import { Button } from '../Button/Button.tsx'

interface Props {
  /** What the download holds: "this sprint's records", "this issue's records". */
  what: string
  onDownload: () => Promise<void>
  open?: boolean
}

/** The records of the chosen scope as one file, collapsed until asked for. */
export function DownloadPanel({ what, onDownload, open = false }: Props) {
  const [preparing, setPreparing] = useState(false)
  const [failed, setFailed] = useState(false)
  const download = async (): Promise<void> => {
    setPreparing(true)
    setFailed(false)
    try {
      await onDownload()
    } catch {
      setFailed(true)
    } finally {
      setPreparing(false)
    }
  }
  return (
    <ActionPanel
      summary={`Download ${what}`}
      about="One JSONL file of every record, for an agent to read or for an audit. Secrets are masked."
      status={preparing ? 'Preparing…' : ''}
      failure={failed ? 'The download failed. Try again.' : null}
      open={open}
    >
      <Button
        disabled={preparing}
        onClick={() => {
          download().catch(() => undefined)
        }}
      >
        Download (.jsonl)
      </Button>
    </ActionPanel>
  )
}
