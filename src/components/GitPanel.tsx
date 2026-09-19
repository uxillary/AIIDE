import { useEffect, useState } from 'react'
import { createGitCommit, getGitCommitDetail, getGitDiff, getGitHistory, getGitStatus, prepareGitCommit, stageGitFile, suggestGitCommitMessage, unstageGitFile } from '../services/git'
import type { CommitDetail, CommitPreview, GitCommit, GitDiff, GitFileStatus, GitStatus } from '../types/git'

const MODEL_KEY = 'aiide.selected-model'
const messageOf = (cause: unknown) => cause instanceof Error ? cause.message : String(cause)

function statusLabel(file: GitFileStatus) {
  if (file.conflict) return 'CONFLICT'
  return file.kind.toUpperCase()
}

export function GitPanel({ projectPath, onStatus }: { projectPath: string; onStatus: (status: GitStatus) => void }) {
  const [status, setStatus] = useState<GitStatus | null>(null)
  const [history, setHistory] = useState<GitCommit[]>([])
  const [selected, setSelected] = useState<{ path: string; staged: boolean } | null>(null)
  const [diff, setDiff] = useState<GitDiff | null>(null)
  const [preview, setPreview] = useState<CommitPreview | null>(null)
  const [message, setMessage] = useState('')
  const [detail, setDetail] = useState<CommitDetail | null>(null)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [notice, setNotice] = useState<string | null>(null)

  async function refresh() {
    const next = await getGitStatus()
    setStatus(next); onStatus(next)
    setHistory(await getGitHistory())
    return next
  }

  useEffect(() => {
    let active = true
    setStatus(null); setHistory([]); setSelected(null); setDiff(null); setPreview(null); setError(null); setNotice(null)
    void Promise.all([getGitStatus(), getGitHistory()]).then(([next, commits]) => {
      if (!active) return
      setStatus(next); setHistory(commits); onStatus(next)
    }).catch(cause => { if (active) setError(messageOf(cause)) })
    return () => { active = false }
  }, [projectPath, onStatus]) // The opened path is the repository identity for this panel.

  async function inspect(file: GitFileStatus, staged = file.staged && !file.unstaged) {
    setSelected({ path: file.path, staged }); setDiff(null); setError(null)
    try { setDiff(await getGitDiff(file.path, staged)) }
    catch (cause) { setError(messageOf(cause)) }
  }

  async function mutate(file: GitFileStatus, stage: boolean) {
    setBusy(true); setError(null); setNotice(null); setPreview(null)
    try {
      const next = stage ? await stageGitFile(file.path) : await unstageGitFile(file.path)
      setStatus(next); onStatus(next); setSelected(null); setDiff(null)
    } catch (cause) { setError(messageOf(cause)) }
    finally { setBusy(false) }
  }

  async function reviewCommit() {
    setBusy(true); setError(null); setNotice(null)
    try { setPreview(await prepareGitCommit()) }
    catch (cause) { setError(messageOf(cause)) }
    finally { setBusy(false) }
  }

  async function suggestMessage() {
    setBusy(true); setError(null)
    try {
      const model = localStorage.getItem(MODEL_KEY) ?? ''
      const suggestion = await suggestGitCommitMessage(model)
      setMessage(suggestion)
    } catch (cause) {
      setError(`${messageOf(cause)} Your existing message was preserved.`)
    } finally { setBusy(false) }
  }

  async function commit() {
    if (!preview || !message.trim()) return
    setBusy(true); setError(null); setNotice(null)
    try {
      const result = await createGitCommit(message, preview.token)
      setNotice(`Committed ${result.hash} · ${result.subject}`); setPreview(null); setMessage(''); setSelected(null); setDiff(null)
      await refresh()
    } catch (cause) {
      setError(messageOf(cause))
      if (messageOf(cause).includes('changed after review')) setPreview(null)
    } finally { setBusy(false) }
  }

  async function showCommit(hash: string) {
    setDetail(null); setError(null)
    try { setDetail(await getGitCommitDetail(hash)) }
    catch (cause) { setError(messageOf(cause)) }
  }

  if (!status && !error) return <section className="git-panel"><h2>GIT WORKTREE</h2><p className="git-muted">Loading Git status…</p></section>
  return <section className="git-panel" aria-label="Git worktree">
    <div className="git-heading"><h2>GIT WORKTREE</h2><button className="git-refresh" disabled={busy} title="Refresh Git status" onClick={() => void refresh().catch(cause => setError(messageOf(cause)))}>↻</button></div>
    {status && <div className="git-branch"><span aria-hidden="true">⑂</span><strong>{status.branch}</strong>{status.detached && <span className="git-warning">DETACHED</span>}</div>}
    {status?.clean ? <div className="git-clean"><span>✓</span><div><strong>Working tree clean</strong><small>Nothing to stage or commit.</small></div></div> : <div className="git-files">
      {status?.files.map(file => <div className={`git-file ${selected?.path === file.path ? 'git-file-selected' : ''}`} key={`${file.path}-${file.originalPath ?? ''}`}>
        <button className="git-file-main" title={file.path} onClick={() => void inspect(file)}>
          <span className={`git-kind git-kind-${file.kind}`}>{statusLabel(file)}</span><span className="git-path">{file.path}{file.originalPath && <small>← {file.originalPath}</small>}</span>
          <span className="git-sides">{file.staged && <b>S</b>}{file.unstaged && <i>U</i>}</span>
        </button>
        <div className="git-file-actions">
          {file.staged && <button disabled={busy} title={`Unstage ${file.path}`} onClick={() => void mutate(file, false)}>− Unstage</button>}
          {file.unstaged && <button disabled={busy} title={`Stage ${file.path}`} onClick={() => void mutate(file, true)}>+ Stage</button>}
          {file.staged && file.unstaged && <button disabled={busy} onClick={() => void inspect(file, true)}>Staged diff</button>}
        </div>
      </div>)}
    </div>}
    {diff && <div className="git-diff"><div><strong>{diff.path}</strong><span>{selected?.staged ? 'STAGED' : 'UNSTAGED'}{diff.binary ? ' · BINARY' : ''}{diff.truncated ? ' · TRUNCATED' : ''}</span></div><pre>{diff.content}</pre></div>}
    {error && <p className="git-error" role="alert">{error}</p>}
    {notice && <p className="git-notice" role="status">{notice}</p>}
    <div className="git-commit-action"><button className="primary-button" disabled={busy || !status || !status.files.some(file => file.staged) || status.hasConflicts || status.detached} onClick={() => void reviewCommit()}>Review commit</button>{status && !status.files.some(file => file.staged) && <small>Stage at least one file to commit.</small>}{status?.hasConflicts && <small>Resolve conflicts before committing.</small>}</div>
    {preview && <div className="commit-review" aria-label="Commit confirmation">
      <div className="pending-badge">READY TO COMMIT · APPROVAL REQUIRED</div><p><span>Branch</span><strong>{preview.branch}</strong></p>
      <div><span>Exact staged files</span><ul>{preview.files.map(path => <li key={path}>{path}</li>)}</ul></div>
      <div><span>Staged summary</span><pre>{preview.summary}</pre></div>
      <label htmlFor="commit-message">Commit message</label><textarea id="commit-message" maxLength={500} value={message} onChange={event => setMessage(event.target.value)} placeholder="Describe the staged changes" />
      <div className="commit-buttons"><button className="small-button" disabled={busy} onClick={() => void suggestMessage()}>✦ Ask Elma</button><button className="primary-button" disabled={busy || !message.trim()} onClick={() => void commit()}>Confirm commit</button><button className="reject-button" disabled={busy} onClick={() => setPreview(null)}>Cancel</button></div>
      <small>AIIDE will re-check the staged state. This never pushes or amends.</small>
    </div>}
    <div className="git-history"><h2>LOCAL COMMITS</h2>{history.length ? history.map(item => <button key={item.hash} onClick={() => void showCommit(item.hash)}><code>{item.hash}</code><span>{item.subject}</span><time>{item.date}</time></button>) : <p className="git-muted">No local commits yet.</p>}{detail && <pre className="commit-detail">{detail.summary || 'No changed-file summary.'}</pre>}</div>
  </section>
}
