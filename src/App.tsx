import { useCallback, useEffect, useRef, useState } from 'react'
import { Elma, type ElmaAnimation, type ElmaLocation, type ElmaPresentation } from './components/Elma'
import { LocalChat } from './components/LocalChat'
import { ChangesPanel, type ChangeHistoryEntry } from './components/ChangesPanel'
import { FileViewer } from './components/FileViewer'
import { ProjectSidebar } from './components/ProjectSidebar'
import { applyPendingChange, chooseProject, refreshProject, rejectPendingChange } from './services/project'
import type { PendingProposal } from './types/ai'
import type { ProjectInfo } from './types/project'
import type { GitStatus } from './types/git'

export default function App() {
  const [project, setProject] = useState<ProjectInfo | null>(null)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [proposal, setProposal] = useState<PendingProposal | null>(null)
  const [changeBusy, setChangeBusy] = useState(false)
  const [changeError, setChangeError] = useState<string | null>(null)
  const [changeState, setChangeState] = useState<'working' | 'success' | 'error' | null>(null)
  const [preparingChanges, setPreparingChanges] = useState(false)
  const [chatElma, setChatElma] = useState<ElmaPresentation>({ state: 'idle', status: 'Ready' })
  const [primaryElma, setPrimaryElma] = useState<{ location: ElmaLocation; phase: 'steady' | 'out' | 'in' }>({ location: 'chat', phase: 'steady' })
  const [history, setHistory] = useState<ChangeHistoryEntry[]>([])
  const [viewedFile, setViewedFile] = useState<string | null>(null)
  const desiredLocation: ElmaLocation = preparingChanges || Boolean(proposal) ? 'changes' : 'chat'
  const desiredLocationRef = useRef(desiredLocation)

  useEffect(() => {
    desiredLocationRef.current = desiredLocation
    setPrimaryElma(current => current.phase === 'steady' && current.location !== desiredLocation ? { ...current, phase: 'out' } : current)
  }, [desiredLocation])

  useEffect(() => {
    if (changeState !== 'success' || primaryElma.location !== 'chat' || primaryElma.phase !== 'steady') return
    const timer = window.setTimeout(() => setChangeState(null), 1300)
    return () => window.clearTimeout(timer)
  }, [changeState, primaryElma])

  const completePortalTransition = useCallback(() => {
    setPrimaryElma(current => {
      if (current.phase === 'out') return { location: desiredLocationRef.current, phase: 'in' }
      if (current.phase === 'in') return current.location === desiredLocationRef.current
        ? { ...current, phase: 'steady' }
        : { ...current, phase: 'out' }
      return current
    })
  }, [])

  async function openProject() {
    setBusy(true)
    setError(null)
    try { const selected = await chooseProject(); if (selected) { setProject(selected); setProposal(null); setPreparingChanges(false); setHistory([]); setViewedFile(null); setChangeError(null); setChangeState(null) } }
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
    } catch (cause) {
      setChangeError(cause instanceof Error ? cause.message : String(cause)); setChangeState('error')
      setTimeout(() => setChangeState(null), 2000)
    } finally { setChangeBusy(false) }
  }

  async function rejectChange() {
    if (!proposal || changeBusy) return
    setChangeBusy(true); setChangeError(null); setChangeState('working')
    try {
      const rejected = proposal
      await rejectPendingChange(); setProposal(null); setChangeState('success')
      const timestamp = Date.now()
      setHistory(current => [{ id: timestamp, outcome: 'rejected', proposal: rejected, timestamp }, ...current])
    }
    catch (cause) { setChangeError(cause instanceof Error ? cause.message : String(cause)); setChangeState('error') }
    finally { setChangeBusy(false) }
  }

  const updateGitSummary = useCallback((status: GitStatus) => {
    setProject(current => current?.repository ? { ...current, repository: { ...current.repository, branch: status.branch, changedFiles: status.files.length } } : current)
  }, [])

  const changesElma: ElmaPresentation = changeState === 'error'
    ? { state: 'error', status: 'Something went wrong' }
    : proposal
      ? { state: 'success', animation: 'changes-ready', status: 'Proposal ready for review' }
      : { state: 'working', animation: 'preparing-changes', status: changeState === 'working' ? 'Applying the approved change…' : 'Preparing changes…' }
  const settledChatElma = changeState === 'success'
    ? { state: 'success', animation: 'changes-applied', status: 'Change resolved' } satisfies ElmaPresentation
    : changeState === 'error'
      ? { state: 'error', status: 'Something went wrong' } satisfies ElmaPresentation
      : chatElma
  const locationElma = primaryElma.location === 'changes' ? changesElma : settledChatElma
  const visibleElma = primaryElma.phase === 'steady'
    ? locationElma
    : { ...locationElma, status: primaryElma.phase === 'out' ? `Moving to ${desiredLocation === 'changes' ? 'Changes' : 'Chat'}…` : `Arriving in ${primaryElma.location === 'changes' ? 'Changes' : 'Chat'}…`, activity: undefined }
  const transitionAnimation: ElmaAnimation | undefined = primaryElma.phase === 'out'
    ? 'portal-out'
    : primaryElma.phase === 'in' ? 'portal-in' : undefined
  const elmaProps = {
    presentation: visibleElma,
    animation: transitionAnimation,
    onAnimationComplete: primaryElma.phase === 'steady' ? undefined : completePortalTransition,
  }

    return (
    <div className="app-shell">
      <header className="topbar">
        <div className="brand">
          <span className="brand-mark">
            <Elma state="idle" size="tiny" animated={false} />
          </span>
          <span className="brand-elma">ELMA</span>
          <span className="brand-heart" aria-hidden="true">♥</span>
          <span className="brand-aiide">AIIDE</span>
          <span className="brand-project">
            {project?.name ?? 'No project open'}
          </span>
        </div>

        <div className="workspace-status">
          <span className="status-dot" />
          LOCAL WORKSPACE
        </div>
      </header>

      <div className="workspace">
        <ProjectSidebar
          project={project}
          onOpen={openProject}
          onViewFile={setViewedFile}
          busy={busy}
        />

        <div className="center-workspace">
          <LocalChat
            projectOpen={Boolean(project)}
            projectBusy={busy}
            onOpenProject={openProject}
            onProposal={next => {
              setProposal(next)
              setPreparingChanges(false)
              setChangeError(null)
            }}
            onChangePreparation={setPreparingChanges}
            onElmaPresentationChange={setChatElma}
            primaryElma={primaryElma.location === 'chat' ? elmaProps : null}
          />

          {viewedFile && (
            <FileViewer
              path={viewedFile}
              onClose={() => setViewedFile(null)}
            />
          )}
        </div>

        <ChangesPanel
          projectPath={project?.path ?? null}
          hasRepository={Boolean(project?.repository)}
          proposal={proposal}
          history={history}
          busy={changeBusy}
          error={changeError}
          primaryElma={primaryElma.location === 'changes' ? elmaProps : null}
          onApply={() => void applyChange()}
          onReject={() => void rejectChange()}
          onGitStatus={updateGitSummary}
        />
      </div>

      {error && (
        <div
          role="alert"
          className="border-t border-red-900 bg-stone-900 px-4 py-2 text-xs text-red-400"
        >
          Could not open project: {error}
        </div>
      )}

      <footer className="footer">
        <span>
          {project
            ? 'LOCAL AI · REVIEWABLE CHANGES'
            : 'LOCAL AI · NO PROJECT OPEN'}
        </span>

        <span>
          {project?.repository
            ? `${project.repository.branch} · ${
                project.repository.changedFiles
                  ? `${project.repository.changedFiles} changed`
                  : 'Clean'
              }`
            : 'No project open'}
        </span>
      </footer>
    </div>
  )
}
