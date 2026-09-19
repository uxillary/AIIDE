import { useState } from 'react'
import type { ImageJob } from '../types/image'

type Outcome = { kind: 'saved'; path: string } | { kind: 'rejected' } | null

export function ImageResultCard({ job, previewUrl, outcome, busy, onCancel, onReject, onRegenerate, onSave }: {
  job: ImageJob
  previewUrl?: string
  outcome: Outcome
  busy: boolean
  onCancel: () => void
  onReject: () => void
  onRegenerate: () => void
  onSave: (relativePath: string) => void
}) {
  const [relativePath, setRelativePath] = useState('assets/generated-image.png')
  const active = job.status === 'submitting' || job.status === 'queued' || job.status === 'generating'
  const canRetry = job.status === 'ready' || job.status === 'failed' || job.status === 'cancelled'
  return <section className={`image-result-card image-result-${job.status}`} aria-label="Generated image">
    <div className="image-result-heading"><span>IMAGE</span><strong>{outcome?.kind === 'saved' ? 'Saved to project' : outcome?.kind === 'rejected' ? 'Rejected' : job.statusLabel}</strong></div>
    {previewUrl && outcome?.kind !== 'rejected' && <img className="image-preview" src={previewUrl} alt={job.prompt} />}
    {!previewUrl && job.status === 'ready' && <div className="image-preview-placeholder">Loading preview…</div>}
    {active && <div className="image-progress" role="status"><span className="connection-dot connected" />{job.statusLabel}</div>}
    {job.error && <p role="alert" className="image-error">{job.error}</p>}
    {outcome?.kind === 'saved' && <p className="image-saved-path">Saved as <code>{outcome.path}</code></p>}
    {!outcome && job.status === 'ready' && <div className="image-save-row"><input aria-label="Project-relative image path" value={relativePath} onChange={event => setRelativePath(event.target.value)} disabled={busy} /><button className="primary-button" disabled={busy || !relativePath.trim()} onClick={() => onSave(relativePath)}>Save to project</button></div>}
    {!outcome && <div className="image-actions">
      {active && job.cancellationSupported && <button className="small-button" disabled={busy} onClick={onCancel}>Cancel</button>}
      {canRetry && <button className="small-button" disabled={busy} onClick={onRegenerate}>Regenerate</button>}
      {!active && <button className="reject-button" disabled={busy} onClick={onReject}>Reject</button>}
    </div>}
    <p className="image-result-meta">SDXL 1.0 · 1024 × 1024 · seed {job.seed}</p>
  </section>
}
