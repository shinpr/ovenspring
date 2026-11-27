// Where each setting lives on the Settings page.

/** The Settings page's sections, in the order the page shows them. */
export const SETTINGS_SECTIONS = [
  ['connections', 'Connections'],
  ['agents', 'Agents'],
  ['merging', 'Merging'],
  ['safety', 'Safety and limits'],
  ['product', 'Running the product'],
] as const

export type SettingsPage = (typeof SETTINGS_SECTIONS)[number][0]

/** The page section that holds a setting the server names by its configuration section. */
const PAGE_OF: Record<string, SettingsPage> = {
  general: 'connections',
  tracker: 'connections',
  slack: 'connections',
  notices: 'connections',
  agents: 'agents',
  roles: 'agents',
  review: 'merging',
  guard: 'safety',
  limits: 'safety',
  app: 'product',
}

export function settingsPath(section: string): string {
  return `/settings/${PAGE_OF[section] ?? 'connections'}`
}
