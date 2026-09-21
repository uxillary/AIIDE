import { useEffect, useState } from 'react'
import { viewRepositoryFile } from '../services/project'

interface FileView { content: string; truncated: boolean }

export function FileViewer({ path, onClose }: { path: string; onClose: () => void }) {
  const [file, setFile] = useState<FileView | null>(null)
  const [error, setError] = useState<string | null>(null)
  const filename = path.split('/').at(-1) ?? path

  useEffect(() => {
    let active = true
    setFile(null); setError(null)
    void viewRepositoryFile(path).then(result => { if (active) setFile(result) }).catch(cause => {
      if (active) setError(cause instanceof Error ? cause.message : String(cause))
    })
    return () => { active = false }
  }, [path])

  useEffect(() => {
    const closeOnEscape = (event: KeyboardEvent) => { if (event.key === 'Escape') onClose() }
    window.addEventListener('keydown', closeOnEscape)
    return () => window.removeEventListener('keydown', closeOnEscape)
  }, [onClose])

  return <section className="file-viewer" aria-label={`Read-only viewer for ${path}`}>
    <header className="file-viewer-header">
      <div><span className="viewer-badge">READ ONLY</span><h2>{filename}</h2><p title={path}>{path}</p></div>
      <button className="icon-button" aria-label="Close file viewer" title="Close file viewer" onClick={onClose}>×</button>
    </header>
    <div className="file-viewer-body">
      {!file && !error && <div className="viewer-state" role="status">Loading file…</div>}
      {error && <div className="viewer-state viewer-error" role="alert"><strong>Unable to display this file</strong><span>{error}</span></div>}
      {file && <>
        {file.truncated && <div className="viewer-warning">Preview truncated at the repository read limit.</div>}
        <ol className="source-lines">{file.content.split('\n').map((line, index) => <li key={index}><code>{line || '\u00a0'}</code></li>)}</ol>
      </>}
    </div>
  </section>
}
