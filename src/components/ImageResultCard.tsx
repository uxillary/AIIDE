import { useEffect, useRef, useState } from 'react'
import type { ImageJob } from '../types/image'

type Outcome = { kind: 'saved'; path: string } | { kind: 'rejected' } | null

export function ImageResultCard({ job, previewUrl, outcome, busy, projectOpen, onCancel, onReject, onRegenerate, onSave, onSaveAs, onReveal }: {
  job: ImageJob
  previewUrl?: string
  outcome: Outcome
  busy: boolean
  projectOpen: boolean
  onCancel: () => void
  onReject: () => void
  onRegenerate: () => void
  onSave: (relativePath: string) => void
  onSaveAs: () => void
  onReveal: (path: string) => void
}) {
  const [relativePath, setRelativePath] = useState('assets/generated-image.png')
  const [viewerOpen, setViewerOpen] = useState(false)
  const previewButtonRef = useRef<HTMLButtonElement>(null)
  useEffect(() => {
    if (!viewerOpen) return
    const close = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        setViewerOpen(false)
        requestAnimationFrame(() => previewButtonRef.current?.focus())
      }
    }
    window.addEventListener('keydown', close)
    return () => window.removeEventListener('keydown', close)
  }, [viewerOpen])
  const active = job.status === 'submitting' || job.status === 'queued' || job.status === 'generating'
  const canRetry = job.status === 'ready' || job.status === 'failed' || job.status === 'cancelled'
  const closeViewer = () => {
    setViewerOpen(false)
    requestAnimationFrame(() => previewButtonRef.current?.focus())
  }
  return <section className={`image-result-card image-result-${job.status}`} aria-label="Generated image">
    <div className="image-result-heading"><span>{outcome ? 'IMAGE' : 'TEMPORARY IMAGE'}</span><strong>{outcome?.kind === 'saved' ? 'Saved' : outcome?.kind === 'rejected' ? 'Rejected' : job.statusLabel}</strong></div>
    {previewUrl && outcome?.kind !== 'rejected' && <button ref={previewButtonRef} className="image-preview-button" onClick={() => setViewerOpen(true)} aria-label="Open full-size image viewer"><img className="image-preview" src={previewUrl} alt={job.prompt} /></button>}
    {!previewUrl && job.status === 'ready' && <div className="image-preview-placeholder">Loading preview…</div>}
    {active && <div className="image-progress" role="status"><span className="connection-dot connected" />{job.statusLabel}</div>}
    {job.error && <p role="alert" className="image-error">{job.error}</p>}
    {outcome?.kind === 'saved' && <div className="image-saved-path">Saved as <code>{outcome.path}</code><button className="small-button" onClick={() => onReveal(outcome.path)}>Reveal in Explorer</button></div>}
    {!outcome && job.status === 'ready' && <div className="image-save-row">{projectOpen && <><input aria-label="Project-relative image path" value={relativePath} onChange={event => setRelativePath(event.target.value)} disabled={busy} /><button className="primary-button" disabled={busy || !relativePath.trim()} onClick={() => onSave(relativePath)}>Save to project</button></>}<button className={projectOpen ? 'small-button' : 'primary-button'} disabled={busy} onClick={onSaveAs}>Save As…</button></div>}
    {!outcome && <div className="image-actions">
      {active && job.cancellationSupported && <button className="small-button" disabled={busy} onClick={onCancel}>Cancel</button>}
      {canRetry && <button className="small-button" disabled={busy} onClick={onRegenerate}>Regenerate</button>}
      {!active && <button className="reject-button" disabled={busy} onClick={onReject}>Reject</button>}
    </div>}
    <p className="image-result-meta">{job.modelDisplayName} · 1024 × 1024 · seed {job.seed}</p>
    {viewerOpen && previewUrl && <div className="image-viewer" role="dialog" aria-modal="true" aria-label="Generated image viewer" onClick={closeViewer}><button className="image-viewer-close" autoFocus onClick={closeViewer} aria-label="Close image viewer">×</button><img src={previewUrl} alt={job.prompt} onClick={event => event.stopPropagation()} /></div>}
  </section>
}
