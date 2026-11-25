import * as a11y from '@storybook/addon-a11y/preview'
import { setProjectAnnotations } from '@storybook/react-vite'
import { inject } from 'vitest'
import * as preview from './preview.tsx'

declare module 'vitest' {
  export interface ProvidedContext {
    theme: 'light' | 'dark'
  }
}

// Each Storybook project of vite.config.ts renders every story in one theme.
setProjectAnnotations([a11y, preview, { initialGlobals: { theme: inject('theme') } }])
