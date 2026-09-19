import { useState } from 'react'
import { Elma } from './components/Elma'
import { LocalChat } from './components/LocalChat'
import { ChangesPanel, type ChangeHistoryEntry } from './components/ChangesPanel'
import { FileViewer } from './components/FileViewer'
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
  const [history, setHistory] = useState<ChangeHistoryEntry[]>([])
  const [viewedFile, setViewedFile] = useState<string | null>(null)

  async function openProject() {
    setBusy(true)
    setError(null)
    try { const selected = await chooseProject(); if (selected) { setProject(selected); setProposal(null); setHistory([]); setViewedFile(null); setChangeError(null); setChangeState(null) } }
    catch (cause) { setError(cause instanceof Error ? cause.message : String(cause)) }
    finally { setBusy(false) }
  }

  async function applyChange() {
    if (!project || !proposal || changeBusy) return
    setChangeBusy(true); setChangeError(null); setChangeState('working')
    try {
      const applied = proposal
      await applyPendingChange()
      const timestamp = Date.now()
      setProposal(null)
      setHistory(current => [{ id: timestamp, outcome: 'applied', proposal: applied, timestamp }, ...current])
      setChangeState('success')
      try { setProject(await refreshProject(project.path)) }
      catch { setChangeError('The change was applied, but project status could not be refreshed.') }
      setTimeout(() => setChangeState(null), 1300)
    } catch (cause) {
      setChangeError(cause instanceof Error ? cause.message : String(cause)); setChangeState('error')
      setTimeout(() => setChangeState(null), 2000)
    } finally { setChangeBusy(false) }
  }

  async function rejectChange() {
    if (!proposal || changeBusy) return
    setChangeBusy(true); setChangeError(null)
    try {
      const rejected = proposal
      await rejectPendingChange(); setProposal(null); setChangeState(null)
      const timestamp = Date.now()
      setHistory(current => [{ id: timestamp, outcome: 'rejected', proposal: rejected, timestamp }, ...current])
    }
    catch (cause) { setChangeError(cause instanceof Error ? cause.message : String(cause)); setChangeState('error') }
    finally { setChangeBusy(false) }
  }

  return <div className="app-shell">
    <header className="topbar"><div className="brand"><span className="brand-mark"><Elma state="idle" size="tiny" /></span><span className="brand-elma">ELMA</span><span className="brand-heart" aria-hidden="true">♥</span><span className="brand-aiide">AIIDE</span><span className="brand-project">{project?.name ?? 'No project open'}</span></div><div className="workspace-status"><span className="status-dot" />LOCAL WORKSPACE</div></header>
    <div className="workspace"><ProjectSidebar project={project} onOpen={openProject} onViewFile={setViewedFile} busy={busy} /><div className="center-workspace"><LocalChat projectOpen={Boolean(project)} projectBusy={busy} onOpenProject={openProject} onProposal={next => { setProposal(next); setChangeError(null) }} changeState={changeState} />{viewedFile && <FileViewer path={viewedFile} onClose={() => setViewedFile(null)} />}</div><ChangesPanel proposal={proposal} history={history} busy={changeBusy} error={changeError} onApply={() => void applyChange()} onReject={() => void rejectChange()} /></div>
    {error && <div role="alert" className="border-t border-red-900 bg-stone-900 px-4 py-2 text-xs text-red-400">Could not open project: {error}</div>}
    <footer className="footer"><span>{project ? 'LOCAL AI · REVIEWABLE CHANGES' : 'LOCAL AI · NO PROJECT OPEN'}</span><span>{project?.repository ? `${project.repository.branch} · ${project.repository.changedFiles ? `${project.repository.changedFiles} changed` : 'Clean'}` : 'No project open'}</span></footer>
  </div>
}
