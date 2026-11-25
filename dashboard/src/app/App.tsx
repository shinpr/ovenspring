import { BrowserRouter, Navigate, Route, Routes } from 'react-router'
import { ActivityPage } from './activity/ActivityPage.tsx'
import { Failed } from './Failed.tsx'
import { GoalPage } from './goal/GoalPage.tsx'
import { IssuePage } from './issue/IssuePage.tsx'
import { PrototypePage } from './issue/PrototypePage.tsx'
import { NowPage } from './now/NowPage.tsx'
import { Shell } from './Shell.tsx'
import { SettingsPage } from './settings/SettingsPage.tsx'
import { SetupPage } from './setup/SetupPage.tsx'
import { SprintsPage } from './sprints/SprintsPage.tsx'

export function App() {
  return (
    <Failed>
      <BrowserRouter>
        <Routes>
          <Route element={<Shell />}>
            <Route index element={<NowPage />} />
            <Route path="setup" element={<SetupPage />} />
            <Route path="goal" element={<GoalPage />} />
            <Route path="issues/:key" element={<IssuePage />} />
            <Route path="issues/:key/prototype" element={<PrototypePage />} />
            <Route path="activity" element={<ActivityPage />} />
            <Route path="sprints" element={<SprintsPage />} />
            <Route path="settings" element={<Navigate to="/settings/connections" replace />} />
            <Route path="settings/:section" element={<SettingsPage />} />
            <Route path="*" element={<Navigate to="/" replace />} />
          </Route>
        </Routes>
      </BrowserRouter>
    </Failed>
  )
}
