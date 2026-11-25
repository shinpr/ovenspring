import { type ReactNode, useCallback, useEffect, useRef, useState } from 'react'
import { ApiError } from '../../api/client.ts'
import type { Cli, SetupCheck, SetupRequest } from '../../api/types.gen.ts'
import { Button } from '../../components/Button/Button.tsx'
import { RadioGroup } from '../../components/Choices/Choices.tsx'
import { ExternalLink } from '../../components/ExternalLink/ExternalLink.tsx'
import { controlClass, Field, SecretInput } from '../../components/Field/Field.tsx'
import { CLI } from '../../lib/vocabulary.ts'
import { MERGERS } from '../merging.ts'
import styles from './Setup.module.css'

interface Props {
  /** The Slack app manifest, as the owner pastes it. */
  manifest: string
  /** This folder's `origin` on GitHub, as `owner/name`. */
  origin: string | null
  /** Checks one block by connecting with its values; a refusal carries the reason under the block's name. */
  onCheck: (block: SetupCheck) => Promise<string>
  /** Saves the connections and the agent; a refusal carries the reasons by block or field. */
  onSave: (request: SetupRequest) => Promise<void>
}

interface Values {
  baseBranch: string
  mergeBy: 'factory' | 'owner'
  githubToken: string
  tracker: 'github' | 'linear'
  team: string
  apiKey: string
  botToken: string
  appToken: string
  channel: string
  owner: string
  cli: Cli | undefined
}

const START: Values = {
  baseBranch: 'main',
  mergeBy: 'factory',
  githubToken: '',
  tracker: 'github',
  team: '',
  apiKey: '',
  botToken: '',
  appToken: '',
  channel: '',
  owner: '',
  cli: undefined,
}

type Block = 'github' | 'tracker' | 'slack'
type Text = Exclude<keyof Values, 'mergeBy' | 'tracker' | 'cli'>

const BLOCKS: Record<Block, string> = { github: 'GitHub', tracker: 'Issues', slack: 'Slack' }
const BLOCK_LIST: readonly Block[] = ['github', 'tracker', 'slack']
const CLIS = ['claude', 'codex', 'opencode'] as const

/** The fields a block needs before it can be checked, with what to say when one is empty. */
function needed(block: Block, v: Values): Array<[Text, string]> {
  switch (block) {
    case 'github':
      return [['baseBranch', 'Enter the base branch.']]
    case 'tracker':
      return v.tracker === 'linear'
        ? [
            ['team', 'Enter the Linear team key.'],
            ['apiKey', 'Enter the Linear API key.'],
          ]
        : []
    case 'slack':
      return [
        ['botToken', 'Enter the bot token.'],
        ['appToken', 'Enter the app-level token.'],
        ['channel', 'Enter the channel ID.'],
        ['owner', 'Enter your Slack member ID.'],
      ]
  }
}

const entered = (v: string): string | null => (v.trim() === '' ? null : v.trim())

function trackerOf(v: Values): SetupRequest['tracker'] {
  return v.tracker === 'linear'
    ? { kind: 'linear', team: v.team.trim(), apiKey: v.apiKey.trim() }
    : { kind: 'github' }
}

function checkOf(block: Block, v: Values): SetupCheck {
  switch (block) {
    case 'github':
      return { section: 'github', token: entered(v.githubToken), baseBranch: v.baseBranch.trim() }
    case 'tracker':
      return { section: 'tracker', tracker: trackerOf(v), githubToken: entered(v.githubToken) }
    case 'slack':
      return {
        section: 'slack',
        botToken: v.botToken.trim(),
        appToken: v.appToken.trim(),
        channel: v.channel.trim(),
        owner: v.owner.trim(),
      }
  }
}

/** The values a block's check is about, or null while one it needs is empty. */
function keyOf(block: Block, v: Values): string | null {
  return needed(block, v).some(([f]) => v[f].trim() === '')
    ? null
    : JSON.stringify(checkOf(block, v))
}

interface Result {
  key: string
  state: 'checking' | 'ok' | 'fail'
  text: string
}

/** How long typing pauses before a complete block is checked. */
const SETTLE_MS = 700

/**
 * Each block's check, run once its values are complete: after typing pauses, at once on leaving a
 * field, or when asked again. Only the newest check of a block counts.
 */
function useChecks(values: Values, onCheck: Props['onCheck']) {
  const [results, setResults] = useState<Partial<Record<Block, Result>>>({})
  const asked = useRef<Partial<Record<Block, string | null>>>({})
  const timers = useRef<Partial<Record<Block, number>>>({})
  const latest = useRef(values)
  latest.current = values

  const run = useCallback(
    (block: Block, v: Values): void => {
      window.clearTimeout(timers.current[block])
      const key = keyOf(block, v)
      asked.current[block] = key
      if (key === null) {
        setResults((r) => ({ ...r, [block]: undefined }))
        return
      }
      setResults((r) => ({ ...r, [block]: { key, state: 'checking', text: '' } }))
      const answer = (state: Result['state'], text: string): void => {
        if (asked.current[block] === key) {
          setResults((r) => ({ ...r, [block]: { key, state, text } }))
        }
      }
      onCheck(checkOf(block, v)).then(
        (found) => answer('ok', found),
        (e: unknown) =>
          answer(
            'fail',
            (e instanceof ApiError ? e.errors[block] : undefined) ?? 'The check could not run.'
          )
      )
    },
    [onCheck]
  )

  // A block complete when the page opens is checked at once; one whose values change is checked
  // once typing pauses; the others keep their result.
  const opened = useRef<boolean>(true)
  const keys = JSON.stringify(BLOCK_LIST.map((b) => keyOf(b, values)))
  useEffect(() => {
    const now: Array<string | null> = JSON.parse(keys)
    const wait = opened.current ? 0 : SETTLE_MS
    opened.current = false
    BLOCK_LIST.forEach((block, i) => {
      if (now[i] !== asked.current[block]) {
        timers.current[block] = window.setTimeout(() => run(block, latest.current), wait)
      }
    })
    return (): void => {
      for (const block of BLOCK_LIST) {
        window.clearTimeout(timers.current[block])
      }
    }
  }, [keys, run])

  /** The result about the values as they are now; none while they are being typed. */
  const current = (block: Block): Result | undefined => {
    const r = results[block]
    return r !== undefined && r.key === keyOf(block, values) ? r : undefined
  }
  /** Checks at once when the values changed since the last check. */
  const settle = (block: Block): void => {
    if (keyOf(block, values) !== asked.current[block]) {
      run(block, values)
    }
  }
  return { current, settle, again: (block: Block): void => run(block, values) }
}

interface Blocker {
  why: string
  /** Its check is still to answer; Continue waits for it. */
  pending: boolean
}

/** What keeps a block from passing, for the list above Continue. */
function blocking(
  block: Block,
  v: Values,
  result: Result | undefined,
  refused: string | undefined
): Blocker[] {
  const empty = needed(block, v).filter(([f]) => v[f].trim() === '')
  if (empty.length > 0) {
    return empty.map(([, why]) => ({ why, pending: false }))
  }
  if (refused !== undefined) {
    return [{ why: refused, pending: false }]
  }
  if (result === undefined || result.state === 'checking') {
    return [{ why: 'being checked now.', pending: true }]
  }
  return result.state === 'fail' ? [{ why: 'the check failed.', pending: false }] : []
}

/** Focuses the first empty field of a block, or the block's first control. */
function focusIn(id: string): void {
  const controls = Array.from(
    document.getElementById(id)?.querySelectorAll<HTMLInputElement>('input') ?? []
  )
  const empty = controls.find((c) => c.type !== 'radio' && c.value.trim() === '')
  ;(empty ?? controls[0])?.focus()
}

interface FieldProps {
  label: string
  hint: ReactNode
  /** Why it is needed, once Continue found it empty. */
  error: string | undefined
  value: string
  onChange: (value: string) => void
  onBlur: () => void
  narrow?: boolean
  secret?: boolean
  optional?: boolean
}

function SetupField({
  label,
  optional = false,
  hint,
  error,
  value,
  onChange,
  onBlur,
  narrow = false,
  secret = false,
}: FieldProps) {
  return (
    <Field label={label} optional={optional} hint={hint} error={error}>
      {(control) =>
        secret ? (
          <SecretInput
            name={label}
            value={value}
            onChange={onChange}
            onBlur={onBlur}
            control={control}
          />
        ) : (
          <input
            {...control}
            className={narrow ? `${controlClass} ${styles['narrow']}` : controlClass}
            spellCheck={false}
            value={value}
            onChange={(e) => onChange(e.target.value)}
            onBlur={onBlur}
          />
        )
      }
    </Field>
  )
}

function CheckResult({
  result,
  refused,
  onAgain,
}: {
  result: Result | undefined
  refused: string | undefined
  onAgain: () => void
}) {
  if (result?.state === 'checking') {
    return (
      <p className={styles['small']}>
        <span className={styles['accent']} aria-hidden="true">
          ●
        </span>{' '}
        Checking…
      </p>
    )
  }
  const fail = refused ?? (result?.state === 'fail' ? result.text : undefined)
  if (fail !== undefined) {
    return (
      <p className={styles['small']}>
        <span className={styles['fail']} aria-hidden="true">
          ✗
        </span>{' '}
        {fail}{' '}
        <Button variant="link" onClick={onAgain}>
          Check again
        </Button>
      </p>
    )
  }
  return result === undefined ? null : (
    <p className={styles['small']}>
      <span className={styles['ok']} aria-hidden="true">
        ✓
      </span>{' '}
      {result.text}
    </p>
  )
}

function BlockSection({
  block,
  title,
  children,
}: {
  block: Block | 'agents'
  title: string
  children: ReactNode
}) {
  return (
    <section
      className={styles['block']}
      id={`setup-${block}`}
      aria-labelledby={`setup-${block}-title`}
    >
      <h3 className={styles['blockTitle']} id={`setup-${block}-title`}>
        {title}
      </h3>
      {children}
    </section>
  )
}

function SlackHowTo({ manifest }: { manifest: string }) {
  const [copied, setCopied] = useState(false)
  return (
    <>
      <p className={styles['small']}>
        Every decision that is yours reaches you here. Create the app once:
      </p>
      <ol className={styles['how']}>
        <li>
          <ExternalLink href="https://api.slack.com/apps?new_app=1">
            Create a Slack app
          </ExternalLink>{' '}
          “From a manifest”, and paste this app's manifest:{' '}
          <Button
            variant="link"
            onClick={() => {
              navigator.clipboard.writeText(manifest).then(
                () => setCopied(true),
                () => undefined
              )
            }}
          >
            copy the manifest
          </Button>
          <span aria-live="polite">{copied ? ' Copied.' : ''}</span>
        </li>
        <li>
          Install it to your workspace, then copy the <b>Bot User OAuth Token</b> (OAuth &amp;
          Permissions).
        </li>
        <li>
          Under Basic Information → App-Level Tokens, create one with <code>connections:write</code>
          .
        </li>
        <li>Invite the app to the channel the factory will use.</li>
      </ol>
    </>
  )
}

/** Connect: GitHub, the issue tracker, Slack and the agent, each connection checked as it is entered. */
export function ConnectView({ manifest, origin, onCheck, onSave }: Props) {
  const [values, setValues] = useState<Values>(START)
  const [attempted, setAttempted] = useState(false)
  const [refused, setRefused] = useState<Record<string, string>>({})
  const [saving, setSaving] = useState(false)
  /** Continue was pressed while checks were still answering: it goes on once they pass. */
  const [waiting, setWaiting] = useState(false)
  const missingRef = useRef<HTMLDivElement>(null)
  // The list renders with the press that shows it, so it takes focus after that render.
  const [focusMissing, setFocusMissing] = useState(0)
  useEffect(() => {
    if (focusMissing > 0) {
      missingRef.current?.focus()
    }
  }, [focusMissing])
  const checks = useChecks(values, onCheck)

  const set =
    <K extends keyof Values>(key: K) =>
    (value: Values[K]): void => {
      setValues((v) => ({ ...v, [key]: value }))
      setRefused({})
    }
  /** A field a block needs: empty after Continue says so; leaving it checks the block. */
  const field = (
    block: Block,
    f: Pick<FieldProps, 'label' | 'hint' | 'narrow' | 'secret'> & { name: Text }
  ) => (
    <SetupField
      label={f.label}
      hint={f.hint}
      error={
        attempted && values[f.name].trim() === ''
          ? needed(block, values).find(([n]) => n === f.name)?.[1]
          : undefined
      }
      value={values[f.name]}
      onChange={set(f.name)}
      onBlur={() => checks.settle(block)}
      narrow={f.narrow ?? false}
      secret={f.secret ?? false}
    />
  )
  // The result's line is always there, so a check starting as a field is left moves nothing below.
  const result = (block: Block) => (
    <div className={styles['status']} aria-live="polite">
      <CheckResult
        result={checks.current(block)}
        refused={refused[block]}
        onAgain={() => checks.again(block)}
      />
    </div>
  )

  const all = BLOCK_LIST.flatMap((block) =>
    blocking(block, values, checks.current(block), refused[block]).map((b) => ({
      ...b,
      target: `setup-${block}`,
      why: `${BLOCKS[block]}: ${b.why}`,
    }))
  )
  if (values.cli === undefined) {
    all.push({ target: 'setup-agents', why: 'Agents: choose the agent.', pending: false })
  }
  const missing = all.filter((b) => !b.pending)
  const pending = all.length - missing.length
  const others = Object.entries(refused).filter(([k]) => !(k in BLOCKS))

  const submit = async (agent: Cli): Promise<void> => {
    setSaving(true)
    try {
      await onSave({
        cli: agent,
        tracker: trackerOf(values),
        slack: {
          botToken: values.botToken.trim(),
          appToken: values.appToken.trim(),
          channel: values.channel.trim(),
          owner: values.owner.trim(),
        },
        repo: { baseBranch: values.baseBranch.trim(), mergeBy: values.mergeBy },
        github: { token: entered(values.githubToken) },
      })
    } catch (e) {
      setRefused(
        e instanceof ApiError && Object.keys(e.errors).length > 0
          ? { ...e.errors }
          : { request: 'Nothing was saved. Try again.' }
      )
      setFocusMissing((n) => n + 1)
    } finally {
      setSaving(false)
    }
  }
  const latestSubmit = useRef(submit)
  latestSubmit.current = submit

  const press = (): void => {
    setAttempted(true)
    for (const block of BLOCK_LIST) {
      checks.settle(block)
    }
    if (missing.length > 0 || values.cli === undefined) {
      setFocusMissing((n) => n + 1)
    } else if (pending > 0) {
      setWaiting(true)
    } else {
      submit(values.cli).catch(() => undefined)
    }
  }
  // Once the checks Continue waited for have answered: on when they all passed, else the list.
  const chosen = values.cli
  useEffect(() => {
    if (!waiting || pending > 0) {
      return
    }
    setWaiting(false)
    if (missing.length === 0 && chosen !== undefined) {
      latestSubmit.current(chosen).catch(() => undefined)
    }
  }, [waiting, pending, missing.length, chosen])
  let label = 'Continue'
  if (waiting) {
    label = 'Checking…'
  } else if (saving) {
    label = 'Saving…'
  }

  return (
    <>
      <div className={styles['intro']}>
        <h2 className={styles['stepTitle']}>Connect</h2>
        <p className={styles['text']}>
          Each connection is checked once its values are in, and nothing is saved until all pass.
          Only <code>.ovenspring/config.toml</code> goes into the repository. Tokens stay outside
          it.
        </p>
      </div>

      <BlockSection block="github" title="GitHub">
        {origin !== null && (
          <p className={styles['small']}>
            This folder's <code>origin</code>: <b>github.com/{origin}</b>
          </p>
        )}
        {field('github', {
          name: 'baseBranch',
          label: 'Base branch',
          hint: 'Issue branches start from it and merge into it.',
          narrow: true,
        })}
        <RadioGroup
          legend="Who merges"
          value={values.mergeBy}
          options={MERGERS.map((m) => ({ ...m, label: <b>{m.label}</b> }))}
          onChange={set('mergeBy')}
        />
        <SetupField
          label="GitHub token"
          optional
          hint="Leave empty to use the signed-in gh. Pull requests are opened as the token's account."
          error={undefined}
          value={values.githubToken}
          onChange={set('githubToken')}
          onBlur={() => {
            checks.settle('github')
            checks.settle('tracker')
          }}
          secret
        />
        {result('github')}
      </BlockSection>

      <BlockSection block="tracker" title="Issues">
        <p className={styles['small']}>Where the factory takes its work from.</p>
        <RadioGroup
          legend="Issue tracker"
          legendHidden
          value={values.tracker}
          options={[
            {
              value: 'github',
              label: <b>GitHub Issues</b>,
              hint: "This repository's issues. Nothing more to enter.",
            },
            {
              value: 'linear',
              label: <b>Linear</b>,
              hint: "A Linear team's Todo and In Progress issues, read with an API key.",
              more: (
                <>
                  {field('tracker', {
                    name: 'team',
                    label: 'Team key',
                    hint: "The letters before an issue's number, such as ACME in ACME-141.",
                    narrow: true,
                  })}
                  {field('tracker', {
                    name: 'apiKey',
                    label: 'API key',
                    hint: 'Linear: Settings → Security & access → Personal API keys.',
                    secret: true,
                  })}
                </>
              ),
            },
          ]}
          onChange={set('tracker')}
        />
        {result('tracker')}
      </BlockSection>

      <BlockSection block="slack" title="Slack">
        <SlackHowTo manifest={manifest} />
        {field('slack', {
          name: 'botToken',
          label: 'Bot token',
          hint: (
            <>
              Starts with <code>xoxb-</code>.
            </>
          ),
          secret: true,
        })}
        {field('slack', {
          name: 'appToken',
          label: 'App-level token',
          hint: (
            <>
              Starts with <code>xapp-</code>.
            </>
          ),
          secret: true,
        })}
        {field('slack', {
          name: 'channel',
          label: 'Channel ID',
          hint: 'Channel details, at the bottom (starts with C).',
          narrow: true,
        })}
        {field('slack', {
          name: 'owner',
          label: 'Your Slack member ID',
          hint: 'Your profile → ⋯ → Copy member ID (starts with U). Only you can approve until you add others in Settings.',
          narrow: true,
        })}
        {result('slack')}
      </BlockSection>

      <BlockSection block="agents" title="Agents">
        <p className={styles['small']}>
          The agent every role runs on, as set up on this computer. You can change it for each role
          in Settings later.
        </p>
        <RadioGroup
          legend="Agent"
          legendHidden
          value={values.cli}
          options={CLIS.map((c) => ({ value: c, label: <b>{CLI[c]}</b> }))}
          onChange={set('cli')}
          after={
            attempted && values.cli === undefined ? (
              <p className={`${styles['small']} ${styles['fail']}`}>Choose the agent.</p>
            ) : undefined
          }
        />
      </BlockSection>

      {attempted && !waiting && (missing.length > 0 || others.length > 0) && (
        <div className={styles['missing']} ref={missingRef} tabIndex={-1} role="alert">
          <p className={styles['blockTitle']}>Before you continue</p>
          <ul>
            {missing.map(({ target, why }) => (
              <li key={why}>
                <a
                  href={`#${target}`}
                  onClick={(e) => {
                    e.preventDefault()
                    focusIn(target)
                  }}
                >
                  {why}
                </a>
              </li>
            ))}
            {others.map(([key, why]) => (
              <li key={key}>{why}</li>
            ))}
          </ul>
        </div>
      )}
      <div className={styles['actions']}>
        <Button variant="primary" disabled={saving || waiting} onClick={press}>
          {label}
        </Button>
      </div>
    </>
  )
}
