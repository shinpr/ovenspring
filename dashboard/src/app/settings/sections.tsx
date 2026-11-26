// The rows of each Settings section. Every row saves the configuration section it belongs to,
// built from the settings as saved with this row's change.

import { Fragment, useState } from 'react'
import type {
  Approval,
  Config,
  Credential,
  GeneralSection,
  Limits,
  Settings,
  SettingsSection,
  Skills,
} from '../../api/types.gen.ts'
import { AgentsSettings } from '../../components/AgentsSettings/AgentsSettings.tsx'
import { RadioGroup } from '../../components/Choices/Choices.tsx'
import { controlClass, Field, monoControlClass } from '../../components/Field/Field.tsx'
import { RoleEditor } from '../../components/RoleEditor/RoleEditor.tsx'
import {
  RunChoice,
  type RunDraft,
  runDraft,
  runRequest,
} from '../../components/RunChoice/RunChoice.tsx'
import { type Reasons, SettingRow } from '../../components/SettingRow/SettingRow.tsx'
import { LinesSetting } from './LinesSetting.tsx'
import { SecretSetting } from './SecretSetting.tsx'
import styles from './Settings.module.css'

export type Save = (section: SettingsSection, body: unknown) => Promise<Reasons>

export interface SectionProps {
  settings: Settings
  skills: Skills
  save: Save
}

/** The `general` section as saved, with `change` applied. */
function general(config: Config, change: Partial<GeneralSection>): GeneralSection {
  return {
    baseBranch: config.repo.base_branch,
    mergeBy: config.merge.by,
    held: Object.fromEntries(Object.entries(config.held).map(([c, h]) => [c, h?.paths ?? []])),
    personReviews: config.person_reviews,
    agreementsPath: config.agreements.path,
    knowledgePath: config.knowledge.path,
    githubToken: null,
    ...change,
  }
}

function savedEnding(settings: Settings, credential: Credential): string | undefined {
  return settings.held.find((h) => h.credential === credential)?.ending
}

function Connected() {
  return <div className={styles['connected']}>Connected when last saved</div>
}

function GitHubRow({ settings, save }: SectionProps) {
  const [token, setToken] = useState('')
  const saved = savedEnding(settings, 'github-token')
  return (
    <SettingRow
      name="GitHub"
      checks
      applies="Checked before saving."
      what="Where the factory pushes, opens pull requests and merges."
      summary={
        <>
          {settings.repository !== null && (
            <div className={styles['line']}>
              Repository <b>{settings.repository}</b> (this folder's <code>origin</code>)
            </div>
          )}
          <div className={styles['line']}>
            {settings.githubAccount === null ? (
              'Acting '
            ) : (
              <>
                Acting as <b>{settings.githubAccount.login}</b>,{' '}
              </>
            )}
            {saved === undefined ? 'with the signed-in gh' : 'with the token you entered'}
          </div>
          <Connected />
        </>
      }
      onOpen={() => setToken('')}
      onSave={() =>
        save('general', general(settings.config, { githubToken: token === '' ? null : token }))
      }
    >
      {() => (
        <SecretSetting
          label="Token"
          help="Pull requests are opened as this token's account. If your branch rules need an approval from someone other than the author, use another account's token."
          saved={saved}
          value={token}
          onChange={setToken}
        />
      )}
    </SettingRow>
  )
}

function TrackerRow({ settings, save }: SectionProps) {
  const saved = settings.config.tracker
  const [kind, setKind] = useState(saved.kind)
  const [team, setTeam] = useState(saved.team ?? '')
  const [key, setKey] = useState('')
  const [filter, setFilter] = useState(saved.filter ?? '')
  return (
    <SettingRow
      name="Issue tracker"
      checks
      applies="Checked before saving."
      what="Where the issues come from, and where the factory marks them started and done."
      summary={
        <>
          {saved.kind === 'linear' ? (
            <>
              <div className={styles['line']}>
                Linear, team <b>{saved.team ?? ''}</b>
              </div>
              <div className={styles['line']}>
                {savedEnding(settings, 'linear-api-key') === undefined
                  ? 'No API key saved'
                  : 'API key set'}
              </div>
            </>
          ) : (
            <div className={styles['line']}>GitHub Issues of this repository</div>
          )}
          <div className={styles['line']}>
            {saved.filter == null ? (
              'Every open issue reaches the agents'
            ) : (
              <>
                Issues filtered by <code>{saved.filter}</code>
              </>
            )}
          </div>
          <Connected />
        </>
      }
      onOpen={() => {
        setKind(saved.kind)
        setTeam(saved.team ?? '')
        setKey('')
        setFilter(saved.filter ?? '')
      }}
      onSave={() =>
        save('tracker', {
          kind,
          team: kind === 'linear' ? team : null,
          filter,
          apiKey: key === '' ? null : key,
        })
      }
      fields={['tracker.team', 'api_key']}
    >
      {(reasons) => (
        <>
          <RadioGroup
            legend="Tracker"
            value={kind}
            options={[
              { value: 'linear', label: 'Linear', hint: 'Todo and In Progress issues are read.' },
              {
                value: 'github',
                label: 'GitHub Issues',
                hint: "This repository's issues. Nothing to enter.",
              },
            ]}
            onChange={setKind}
          />
          {kind === 'linear' && (
            <>
              <Field
                label="Team key"
                hint="The letters before an issue's number, such as ACME in ACME-141."
                error={reasons['tracker.team']}
              >
                {(control) => (
                  <input
                    {...control}
                    className={controlClass}
                    value={team}
                    onChange={(e) => setTeam(e.target.value)}
                  />
                )}
              </Field>
              <SecretSetting
                label="API key"
                help="Create one in Linear: Settings → Security & access → Personal API keys."
                saved={savedEnding(settings, 'linear-api-key')}
                value={key}
                onChange={setKey}
                error={reasons['api_key']}
              />
            </>
          )}
          <Field
            label="Filter command"
            hint="Runs on each new or changed issue before any agent reads it, so the issues it rejects never reach the agents. Leave empty for no filter. What the command receives and how its exit code decides are in the guide, under Tracker issue filter."
          >
            {(control) => (
              <input
                {...control}
                className={monoControlClass}
                value={filter}
                onChange={(e) => setFilter(e.target.value)}
              />
            )}
          </Field>
        </>
      )}
    </SettingRow>
  )
}

function SlackRow({ settings, save }: SectionProps) {
  const saved = settings.config.slack
  const [channel, setChannel] = useState(saved.channel)
  const [bot, setBot] = useState('')
  const [app, setApp] = useState('')
  return (
    <SettingRow
      name="Slack"
      checks
      applies="Checked before saving."
      what="Where every decision reaches you and where you talk with the lead."
      summary={
        <>
          <div className={styles['line']}>
            Channel <code>{saved.channel}</code>
          </div>
          <div className={styles['line']}>
            {savedEnding(settings, 'slack-bot-token') !== undefined &&
            savedEnding(settings, 'slack-app-token') !== undefined
              ? 'Bot token and app-level token set'
              : 'A token is missing'}
          </div>
          <Connected />
        </>
      }
      onOpen={() => {
        setChannel(saved.channel)
        setBot('')
        setApp('')
      }}
      onSave={() =>
        save('slack', {
          channel,
          owners: saved.owners,
          botToken: bot === '' ? null : bot,
          appToken: app === '' ? null : app,
        })
      }
      fields={['slack.channel']}
    >
      {(reasons) => (
        <>
          <Field
            label="Channel ID"
            hint="At the bottom of the channel's details (starts with C). Invite the app to the channel first."
            error={reasons['slack.channel']}
          >
            {(control) => (
              <input
                {...control}
                className={controlClass}
                value={channel}
                onChange={(e) => setChannel(e.target.value)}
              />
            )}
          </Field>
          <SecretSetting
            label="Bot token"
            help={
              <>
                Slack app → OAuth &amp; Permissions → Bot User OAuth Token (<code>xoxb-</code>).
              </>
            }
            saved={savedEnding(settings, 'slack-bot-token')}
            value={bot}
            onChange={setBot}
          />
          <SecretSetting
            label="App-level token"
            help={
              <>
                Slack app → Basic Information → App-Level Tokens, with{' '}
                <code>connections:write</code> (<code>xapp-</code>).
              </>
            }
            saved={savedEnding(settings, 'slack-app-token')}
            value={app}
            onChange={setApp}
          />
        </>
      )}
    </SettingRow>
  )
}

function OwnersRow({ settings, save }: SectionProps) {
  const saved = settings.config.slack
  const [text, setText] = useState(saved.owners.join('\n'))
  const owners = text.split('\n').filter((o) => o.trim() !== '')
  return (
    <SettingRow
      name="Who the factory answers"
      applies="Applies at once."
      what="Only these Slack members can approve, answer and pause."
      summary={saved.owners.map((o) => (
        <div key={o} className={styles['line']}>
          <code>{o}</code>
        </div>
      ))}
      mono
      onOpen={() => setText(saved.owners.join('\n'))}
      onSave={() =>
        save('slack', { channel: saved.channel, owners, botToken: null, appToken: null })
      }
      fields={['slack.owners']}
    >
      {(reasons) => (
        <Field
          label="Slack member IDs, one per line"
          hint="In Slack, open a person's profile → ⋯ → Copy member ID (starts with U). If you remove yourself, the factory stops answering you and your approvals stop counting."
          error={reasons['slack.owners']}
        >
          {(control) => (
            <textarea
              {...control}
              className={monoControlClass}
              rows={3}
              value={text}
              onChange={(e) => setText(e.target.value)}
            />
          )}
        </Field>
      )}
    </SettingRow>
  )
}

export function Connections(props: SectionProps) {
  return (
    <>
      <GitHubRow {...props} />
      <TrackerRow {...props} />
      <SlackRow {...props} />
      <OwnersRow {...props} />
    </>
  )
}

export function Agents({ settings, skills, save }: SectionProps) {
  return (
    <>
      <li className={styles['subgroup']}>
        <h3 className={styles['subgroupTitle']}>Roles</h3>
        <p className={styles['subgroupText']}>Each role runs on its own agent.</p>
      </li>
      {(['lead', 'executor', 'reviewer', 'analyzer'] as const).map((role) => (
        <RoleEditor
          key={role}
          role={role}
          config={settings.config.roles[role]}
          skills={skills}
          onSave={(section) => save('roles', section)}
        />
      ))}
      <AgentsSettings
        agents={settings.config.agents}
        available={settings.mcp}
        onSave={(agents) => save('agents', agents)}
      />
      <FolderRow
        settings={settings}
        save={save}
        name="Working agreements folder"
        what="Where the factory keeps what it learned about its own work."
        saved={settings.config.agreements.path}
        field="agreements.path"
        change={(path) => ({ agreementsPath: path })}
        example="docs/factory/agreements"
      />
      <FolderRow
        settings={settings}
        save={save}
        name="Knowledge folder"
        what="Where the factory keeps what it learns about this product."
        saved={settings.config.knowledge.path}
        field="knowledge.path"
        change={(path) => ({ knowledgePath: path })}
        example="docs/knowledge"
      />
    </>
  )
}

function FolderRow({
  settings,
  save,
  name,
  what,
  saved,
  field,
  change,
  example,
}: Pick<SectionProps, 'settings' | 'save'> & {
  name: string
  what: string
  saved: string
  field: string
  change: (path: string) => Partial<GeneralSection>
  example: string
}) {
  const [path, setPath] = useState(saved)
  return (
    <SettingRow
      name={name}
      what={what}
      summary={<code>{saved}</code>}
      onOpen={() => setPath(saved)}
      onSave={() => save('general', general(settings.config, change(path)))}
      fields={[field]}
    >
      {(reasons) => (
        <Field
          label="Folder in the repository"
          hint={`A folder git tracks, such as ${example}.`}
          error={reasons[field]}
        >
          {(control) => (
            <input
              {...control}
              className={monoControlClass}
              value={path}
              onChange={(e) => setPath(e.target.value)}
            />
          )}
        </Field>
      )}
    </SettingRow>
  )
}

/** The categories held by path, with what each holds. */
const CATEGORIES: ReadonlyArray<{
  category: Approval
  name: string
  what: string
  example: string
}> = [
  {
    category: 'schema',
    name: 'Schema changes',
    what: 'Database migrations and schema files.',
    example: 'db/migrations/**',
  },
  {
    category: 'external-connection',
    name: 'External connections',
    what: 'A new service the product calls or is called by.',
    example: 'src/clients/**',
  },
  {
    category: 'infrastructure',
    name: 'Infrastructure',
    what: 'Deployment and hosting files.',
    example: 'terraform/**',
  },
  {
    category: 'ci',
    name: 'CI changes',
    what: 'The checks that guard merging.',
    example: '.github/workflows/**',
  },
  {
    category: 'external-contract',
    name: 'External contracts',
    what: 'Interfaces others depend on.',
    example: 'openapi/**',
  },
]

function HeldRow({
  settings,
  save,
  category,
  name,
  what,
  example,
}: SectionProps & (typeof CATEGORIES)[number]) {
  const saved = settings.config.held[category]?.paths ?? []
  const key = `held.${category}.paths`
  return (
    <LinesSetting
      name={name}
      what={what}
      none="No paths: held only when a design declares it"
      label="Path patterns, one per line"
      hint={
        <>
          <code>*</code> within a folder name, <code>**</code> across folders, such as{' '}
          <code>{example}</code>.
        </>
      }
      field={key}
      values={saved}
      onSave={(paths) => {
        const held = general(settings.config, {}).held
        return save('general', general(settings.config, { held: { ...held, [category]: paths } }))
      }}
    />
  )
}

export function Merging(props: SectionProps) {
  const { settings, save } = props
  const config = settings.config
  const [branch, setBranch] = useState(config.repo.base_branch)
  const [mergeBy, setMergeBy] = useState(config.merge.by)
  return (
    <>
      <SettingRow
        name="Base branch"
        checks
        applies="Checked on GitHub before saving."
        what="Issue branches start from it and merge into it."
        summary={<code>{config.repo.base_branch}</code>}
        onOpen={() => setBranch(config.repo.base_branch)}
        onSave={() => save('general', general(config, { baseBranch: branch }))}
        fields={['repo.base_branch']}
      >
        {(reasons) => (
          <Field label="Branch" error={reasons['repo.base_branch']}>
            {(control) => (
              <input
                {...control}
                className={controlClass}
                value={branch}
                onChange={(e) => setBranch(e.target.value)}
              />
            )}
          </Field>
        )}
      </SettingRow>
      <SettingRow
        name="Who merges"
        what="When every condition holds, who presses merge."
        summary={
          config.merge.by === 'factory' ? (
            <>
              <b>The factory</b>, once checks and every approval are in.
            </>
          ) : (
            <>
              <b>You</b>. The factory tells you in Slack when a pull request is ready.
            </>
          )
        }
        onOpen={() => setMergeBy(config.merge.by)}
        onSave={() => save('general', general(config, { mergeBy }))}
      >
        {() => (
          <RadioGroup
            legend="Who merges"
            legendHidden
            value={mergeBy}
            options={[
              {
                value: 'factory',
                label: <b>The factory</b>,
                hint: 'It merges the exact commit that passed.',
              },
              {
                value: 'owner',
                label: <b>You</b>,
                hint: 'The factory tells you in Slack when a pull request is ready.',
              },
            ]}
            onChange={setMergeBy}
          />
        )}
      </SettingRow>
      <SettingRow
        name="Always held for you"
        what="Whatever you set below, these wait for your approval in Slack before the factory merges."
        summary={
          <>
            <div className={styles['line']}>
              Changes to <code>.ovenspring/</code>, the working agreements, the knowledge,{' '}
              <code>AGENTS.md</code> or <code>CLAUDE.md</code>
            </div>
            <div className={styles['line']}>
              Anything a design said it adds: schema, external connections, infrastructure, CI or
              external contracts
            </div>
          </>
        }
      />
      <PersonReviews {...props} />
      <ReviewBots {...props} />
      <li className={styles['subgroup']}>
        <h3 className={styles['subgroupTitle']}>
          Also held when a pull request changes these files
        </h3>
        <p className={styles['subgroupText']}>Even if no design said so.</p>
        <ul className={styles['rows']}>
          {CATEGORIES.map((c) => (
            <HeldRow key={c.category} {...props} {...c} />
          ))}
        </ul>
      </li>
      {config.merge.by === 'owner' && (
        <li className={styles['subgroupText']}>
          You merge every pull request yourself, so nothing is held separately.
        </li>
      )}
    </>
  )
}

/** `name: words` as the owner writes it, one kind of change per line. */
function namedLine(line: string): [string, string] {
  const at = line.indexOf(':')
  return at < 0 ? [line.trim(), ''] : [line.slice(0, at).trim(), line.slice(at + 1).trim()]
}

function PersonReviews({ settings, save }: SectionProps) {
  const config = settings.config
  return (
    <LinesSetting
      name="Changes a person reviews"
      what={
        config.merge.by === 'factory'
          ? 'Each waits for your approval before the factory merges it.'
          : 'The ready-to-merge message names each, so you know where to look.'
      }
      none="None"
      label="One per line: a name, a colon, then the changes it covers"
      hint={
        <>
          Such as <code>payments: how customers are charged or refunded</code>.
        </>
      }
      field="person_reviews"
      values={Object.entries(config.person_reviews).map(([name, words]) => `${name}: ${words}`)}
      show={(line) => {
        const [name, words] = namedLine(line)
        return (
          <>
            <b>{name}</b>: {words}
          </>
        )
      }}
      onSave={(lines) =>
        save(
          'general',
          general(config, { personReviews: Object.fromEntries(lines.map(namedLine)) })
        )
      }
    />
  )
}

function ReviewBots({ settings, save }: SectionProps) {
  const saved = settings.config.review.bots
  return (
    <LinesSetting
      name="Review bots"
      what="Their comments are judged like a write-access reviewer's. Comments from other accounts without write access never reach an agent."
      none="None: only accounts with write access are answered"
      label="Bot accounts, one per line"
      hint={
        <>
          As GitHub shows the author, such as <code>ai-reviewer[bot]</code>.
        </>
      }
      field="review.bots"
      values={saved}
      onSave={(bots) => save('review', { bots })}
    />
  )
}

function Denied({ settings, save }: SectionProps) {
  const saved = settings.config.guard.deny
  return (
    <LinesSetting
      name="Denied commands"
      what="Agents are stopped when they try to run these."
      before={
        <div className={styles['always']}>
          Always denied:{' '}
          {settings.alwaysDenied.map((d, i) => (
            <Fragment key={d.text}>
              {i > 0 && (i === settings.alwaysDenied.length - 1 ? ', and ' : ', ')}
              {d.kind === 'command' ? <code>{d.text}</code> : d.text}
            </Fragment>
          ))}
          .
        </div>
      }
      none="None of your own"
      label="Your denied commands, one pattern per line"
      hint={
        <>
          <code>*</code> matches any text. Each command of a chained line is checked on its own.
          This catches mistakes. To keep agents out of production, keep production credentials off
          this computer.
        </>
      }
      field="guard.deny"
      values={saved}
      onSave={(deny) => save('guard', { deny })}
    />
  )
}

function count(text: string, optional: boolean): number | null | 'invalid' {
  const entered = text.trim()
  if (entered === '' && optional) {
    return null
  }
  const n = Number(entered)
  return entered !== '' && Number.isInteger(n) && n >= 0 ? n : 'invalid'
}

export function Safety(props: SectionProps) {
  const { settings, save } = props
  const limits = settings.config.limits
  const [atOnce, setAtOnce] = useState(String(limits.issues_at_once))
  const [review, setReview] = useState(String(limits.review_rounds ?? ''))
  const [fix, setFix] = useState(String(limits.fix_ci_rounds ?? ''))
  const rounds = (v: number | null | undefined) =>
    v == null ? <span className={styles['muted']}>no limit</span> : v
  const saveLimits = (change: Partial<Limits>): Promise<Reasons> =>
    save('limits', { ...limits, ...change })
  return (
    <>
      <Denied {...props} />
      <SettingRow
        name="Issues at once"
        what="How many issues are under way at the same time; the others wait to start."
        summary={<b>{limits.issues_at_once}</b>}
        onOpen={() => setAtOnce(String(limits.issues_at_once))}
        onSave={async () => {
          const n = count(atOnce, false)
          return n === 'invalid' || n === null
            ? { 'limits.issues_at_once': 'Enter a whole number.' }
            : saveLimits({ issues_at_once: n })
        }}
        fields={['limits.issues_at_once']}
      >
        {(reasons) => (
          <Field
            label="Issues at once"
            hint="1 or more. More finishes sooner and uses your agents' plans faster."
            error={reasons['limits.issues_at_once']}
          >
            {(control) => (
              <input
                {...control}
                className={`${controlClass} ${styles['number']}`}
                inputMode="numeric"
                value={atOnce}
                onChange={(e) => setAtOnce(e.target.value)}
              />
            )}
          </Field>
        )}
      </SettingRow>
      <SettingRow
        name="Round limits"
        what="After this many review rounds or CI fixes on one issue, the lead takes it as a problem instead of trying again."
        summary={
          <>
            <div className={styles['line']}>Review rounds: {rounds(limits.review_rounds)}</div>
            <div className={styles['line']}>CI fixes: {rounds(limits.fix_ci_rounds)}</div>
          </>
        }
        onOpen={() => {
          setReview(String(limits.review_rounds ?? ''))
          setFix(String(limits.fix_ci_rounds ?? ''))
        }}
        onSave={async () => {
          const r = count(review, true)
          const f = count(fix, true)
          if (r === 'invalid' || f === 'invalid') {
            return { request: 'Enter a whole number, or leave it empty for no limit.' }
          }
          return saveLimits({ review_rounds: r, fix_ci_rounds: f })
        }}
      >
        {() => (
          <>
            <Field label="Review rounds" hint="Empty for no limit.">
              {(control) => (
                <input
                  {...control}
                  className={`${controlClass} ${styles['number']}`}
                  inputMode="numeric"
                  value={review}
                  onChange={(e) => setReview(e.target.value)}
                />
              )}
            </Field>
            <Field label="CI fixes" hint="Empty for no limit.">
              {(control) => (
                <input
                  {...control}
                  className={`${controlClass} ${styles['number']}`}
                  inputMode="numeric"
                  value={fix}
                  onChange={(e) => setFix(e.target.value)}
                />
              )}
            </Field>
          </>
        )}
      </SettingRow>
    </>
  )
}

const RUN_LABELS = [
  ['Prepare', 'setup'],
  ['Start', 'start'],
  ['Ready when this answers', 'ready'],
] as const

/** How the product runs: the commands each issue's copy starts with, or nothing to run. */
export function Product({ settings, save }: SectionProps) {
  const app = settings.config.app
  const [draft, setDraft] = useState<RunDraft>(() => runDraft(app, app.nothing_to_run === true))
  return (
    <SettingRow
      name="Commands"
      checks
      applies="Starts the product once to check. Applies from the next start."
      what="Checked by starting the product once."
      summary={
        app.nothing_to_run === true ? (
          <>
            <b>Nothing to run.</b>{' '}
            <span className={styles['muted']}>
              Agents check their changes with the tests only, and Slack offers no preview.
            </span>
          </>
        ) : (
          RUN_LABELS.flatMap(([label, key]) => {
            const command = app[key]
            return command == null
              ? []
              : [
                  <div key={key} className={styles['line']}>
                    {label}: <code>{command}</code>
                  </div>,
                ]
          })
        )
      }
      onOpen={() => setDraft(runDraft(app, app.nothing_to_run === true))}
      onSave={() => save('app', runRequest(draft))}
      fields={['start', 'check']}
    >
      {(reasons) => (
        <RunChoice
          draft={draft}
          onChange={setDraft}
          startError={reasons['start']}
          status={
            reasons['check'] === undefined ? undefined : (
              <>
                <p className={styles['refused']} role="alert">
                  Did not start. What it printed last:
                </p>
                <pre className={styles['out']}>{reasons['check']}</pre>
              </>
            )
          }
        />
      )}
    </SettingRow>
  )
}
