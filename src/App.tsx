import { useState } from 'react'
import { LocalChat } from './components/LocalChat'
import { ChangesPanel } from './components/ChangesPanel'
import { ProjectSidebar } from './components/ProjectSidebar'
import { applyPendingChange, chooseProject, refreshProject, rejectPendingChange } from './services/project'
import type { PendingProposal } from './types/ai'
import type { ProjectInfo } from './types/project'

export default function App() {
  const [project, setProject] = useState<ProjectInfo | null>(null)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [proposal, setProposal] = useState<PendingProposal | null>(null)
  const [changeBusy, setChangeBusy] = useState(false)
  const [changeError, setChangeError] = useState<string | null>(null)
  const [changeState, setChangeState] = useState<'working' | 'success' | 'error' | null>(null)

  async function openProject() {
    setBusy(true)
    setError(null)
    try { const selected = await chooseProject(); if (selected) { setProject(selected); setProposal(null); setChangeError(null) } }
    catch (cause) { setError(cause instanceof Error ? cause.message : String(cause)) }
    finally { setBusy(false) }
  }

  async function applyChange() {
    if (!project || !proposal || changeBusy) return
    setChangeBusy(true); setChangeError(null); setChangeState('working')
    try {
      await applyPendingChange(); setProposal(null); setProject(await refreshProject(project.path)); setChangeState('success')
      setTimeout(() => setChangeState(null), 1300)
    } catch (cause) {
      setChangeError(cause instanceof Error ? cause.message : String(cause)); setChangeState('error')
      setTimeout(() => setChangeState(null), 2000)
    } finally { setChangeBusy(false) }
  }

  async function rejectChange() {
    if (!proposal || changeBusy) return
    setChangeBusy(true); setChangeError(null)
    try { await rejectPendingChange(); setProposal(null); setChangeState(null) }
    catch (cause) { setChangeError(cause instanceof Error ? cause.message : String(cause)); setChangeState('error') }
    finally { setChangeBusy(false) }
  }

  return <div className="app-shell">
    <header className="topbar"><div className="flex items-center gap-3"><span className="brand-mark">A</span><span className="text-sm font-semibold tracking-[0.16em] text-stone-100">AIIDE</span><span className="hidden text-xs text-stone-600 sm:inline">/</span><span className="hidden truncate text-xs text-stone-400 sm:inline">{project?.name ?? 'No project open'}</span></div><div className="flex items-center gap-2 text-xs text-stone-500"><span className="status-dot" />LOCAL WORKSPACE</div></header>
    <div className="workspace"><ProjectSidebar project={project} onOpen={openProject} busy={busy} /><LocalChat projectOpen={Boolean(project)} onProposal={next => { setProposal(next); setChangeError(null) }} changeState={changeState} /><ChangesPanel proposal={proposal} busy={changeBusy} error={changeError} onApply={() => void applyChange()} onReject={() => void rejectChange()} /></div>
    {error && <div role="alert" className="border-t border-red-900 bg-stone-900 px-4 py-2 text-xs text-red-400">Could not open project: {error}</div>}
    <footer className="footer"><span>{project ? 'LOCAL AI · REVIEWABLE CHANGES' : 'LOCAL AI · NO PROJECT OPEN'}</span><span>{project?.repository ? `${project.repository.branch} · ${project.repository.changedFiles ? `${project.repository.changedFiles} changed` : 'Clean'}` : 'No project open'}</span></footer>
  </div>
}
