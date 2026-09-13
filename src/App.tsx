import { useState } from 'react'
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
    <div className="workspace"><ProjectSidebar project={project} onOpen={openProject} busy={busy} /><main className="main-panel"><div className="main-label">WORKSPACE</div><div className="flex flex-1 items-center justify-center px-6 py-12"><div className="max-w-md text-center"><div className="mx-auto mb-6 flex h-12 w-12 items-center justify-center rounded-xl border border-stone-700 bg-stone-800/70 text-xl font-semibold text-stone-100">A</div><h1 className="text-2xl font-semibold tracking-tight text-stone-100">{project ? project.name : 'AIIDE'}</h1><p className="mt-2 text-sm text-stone-400">{project ? 'Project opened. Explore its files and Git state in the sidebar.' : 'Your local AI coding workspace.'}</p><p className="mt-4 text-xs leading-5 text-stone-500">{project ? 'AI features are planned for a later milestone.' : 'Open a folder to inspect it locally. Future milestones will add controlled AI-assisted repository work.'}</p><button onClick={openProject} disabled={busy} className="primary-button mt-7">{busy ? 'Opening…' : project ? 'Open another project' : 'Open project'}</button>{error && <p role="alert" className="mt-4 text-xs text-red-400">{error}</p>}</div></div></main><aside className="changes-panel"><div className="main-label">CHANGES</div><div className="px-4 pt-5 text-xs leading-5 text-stone-500">Change review will arrive in a later milestone.</div></aside></div>
    <footer className="footer"><span>READ-ONLY PROJECT VIEW</span><span>{project?.repository ? `${project.repository.branch} · ${project.repository.changedFiles ? `${project.repository.changedFiles} changed` : 'Clean'}` : 'Local first'}</span></footer>
  </div>
}
