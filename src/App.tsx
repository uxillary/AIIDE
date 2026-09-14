import { useState } from 'react'
import { LocalChat } from './components/LocalChat'
import { ProjectSidebar } from './components/ProjectSidebar'
import { chooseProject } from './services/project'
import type { ProjectInfo } from './types/project'

export default function App() {
  const [project, setProject] = useState<ProjectInfo | null>(null)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)

  async function openProject() {
    setBusy(true)
    setError(null)
    try { const selected = await chooseProject(); if (selected) setProject(selected) }
    catch (cause) { setError(cause instanceof Error ? cause.message : String(cause)) }
    finally { setBusy(false) }
  }

  return <div className="app-shell">
    <header className="topbar"><div className="flex items-center gap-3"><span className="brand-mark">A</span><span className="text-sm font-semibold tracking-[0.16em] text-stone-100">AIIDE</span><span className="hidden text-xs text-stone-600 sm:inline">/</span><span className="hidden truncate text-xs text-stone-400 sm:inline">{project?.name ?? 'No project open'}</span></div><div className="flex items-center gap-2 text-xs text-stone-500"><span className="status-dot" />LOCAL WORKSPACE</div></header>
    <div className="workspace"><ProjectSidebar project={project} onOpen={openProject} busy={busy} /><LocalChat projectOpen={Boolean(project)} /><aside className="changes-panel"><div className="main-label">CHANGES</div><div className="px-4 pt-5 text-xs leading-5 text-stone-500">Change review will arrive in a later milestone.</div></aside></div>
    {error && <div role="alert" className="border-t border-red-900 bg-stone-900 px-4 py-2 text-xs text-red-400">Could not open project: {error}</div>}
    <footer className="footer"><span>{project ? 'LOCAL AI · READ-ONLY PROJECT ACCESS' : 'LOCAL AI · NO PROJECT OPEN'}</span><span>{project?.repository ? `${project.repository.branch} · ${project.repository.changedFiles ? `${project.repository.changedFiles} changed` : 'Clean'}` : 'No project open'}</span></footer>
  </div>
}
