// The theme the owner chose: `light`, `dark`, or `auto` (follows the system). It is kept per browser;
// where storage is unavailable (a private window, blocked site data) the choice holds for the page
// only, which the owner sees as the theme returning to Auto on reload.

export type ThemeChoice = 'light' | 'dark' | 'auto'

export const THEME_CHOICES: readonly ThemeChoice[] = ['light', 'dark', 'auto']

const KEY = 'ovenspring-theme'

export function readTheme(): ThemeChoice {
  try {
    const stored = window.localStorage.getItem(KEY)
    return stored === 'light' || stored === 'dark' ? stored : 'auto'
  } catch {
    return 'auto'
  }
}

export function chooseTheme(choice: ThemeChoice): void {
  applyTheme(choice)
  try {
    if (choice === 'auto') {
      window.localStorage.removeItem(KEY)
    } else {
      window.localStorage.setItem(KEY, choice)
    }
  } catch {
    // Storage unavailable: the choice holds for this page only (see above).
  }
}

export function applyTheme(choice: ThemeChoice): void {
  const root = document.documentElement
  if (choice === 'auto') {
    root.removeAttribute('data-theme')
  } else {
    root.setAttribute('data-theme', choice)
  }
}
