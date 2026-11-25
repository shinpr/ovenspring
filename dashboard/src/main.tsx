import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import { App } from './app/App.tsx'
import { applyTheme, readTheme } from './theme.ts'
import './styles/index.ts'

applyTheme(readTheme())

const root = document.getElementById('root')
if (root === null) {
  throw new Error('index.html has no #root element')
}
createRoot(root).render(
  <StrictMode>
    <App />
  </StrictMode>
)
