import type { PendingChange, PendingProposal } from '../types/ai'
import { Elma } from './Elma'
import { GitPanel } from './GitPanel'
import type { GitStatus } from '../types/git'

export interface ChangeHistoryEntry {
  id: number
  outcome: 'applied' | 'rejected'
  proposal: PendingProposal
  timestamp: number
}

function diff(change: PendingChange) {
  const before = change.before.split('\n')
  const after = change.after.split('\n')
  let start = 0
  while (start < before.length && start < after.length && before[start] === after[start]) start++
  let beforeEnd = before.length - 1
  let afterEnd = after.length - 1
  while (beforeEnd >= start && afterEnd >= start && before[beforeEnd] === after[afterEnd]) { beforeEnd--; afterEnd-- }
  const contextStart = Math.max(0, start - 2)
  const contextEndAfter = Math.min(after.length - 1, afterEnd + 2)
  return [
    ...(contextStart > 0 ? [{ kind: 'meta', text: `@@ around line ${start + 1} @@` }] : []),
    ...before.slice(contextStart, start).map(text => ({ kind: 'context', text })),
    ...before.slice(start, beforeEnd + 1).map(text => ({ kind: 'delete', text })),
    ...after.slice(start, afterEnd + 1).map(text => ({ kind: 'add', text })),
    ...after.slice(afterEnd + 1, contextEndAfter + 1).map(text => ({ kind: 'context', text })),
  ]
}

function ProposalDiff({ proposal }: { proposal: PendingProposal }) {
  return <>{proposal.changes.map(change => <section className="change-section" key={change.path}><div className="change-file"><span aria-hidden="true">⌘</span> {change.path}</div><pre className="change-diff">{diff(change).map((line, index) => <div key={index} className={`diff-${line.kind}`}><span>{line.kind === 'delete' ? '-' : line.kind === 'add' ? '+' : ' '}</span>{line.text}</div>)}</pre></section>)}</>
}

export function ChangesPanel({ projectPath, hasRepository, proposal, history, busy, error, onApply, onReject, onGitStatus }: { projectPath: string | null; hasRepository: boolean; proposal: PendingProposal | null; history: ChangeHistoryEntry[]; busy: boolean; error: string | null; onApply: () => void; onReject: () => void; onGitStatus: (status: GitStatus) => void }) {
  return <aside className="changes-panel" aria-label="Changes"><div className="main-label">CHANGES</div><section className="proposal-panel" aria-label="AiiDE proposal changes"><h2 className="panel-section-label">AIIDE PROPOSALS</h2>{proposal ? <div className="changes-review">
    <div className="pending-badge">READY FOR REVIEW · NOT APPLIED</div><p className="change-summary">{proposal.summary}</p>
    <ProposalDiff proposal={proposal} />
    {error && <p role="alert" className="change-error">{error}</p>}
    <div className="change-actions"><button className="primary-button" disabled={busy} onClick={onApply}><span aria-hidden="true">✓</span> Apply</button><button className="reject-button" disabled={busy} onClick={onReject}><span aria-hidden="true">×</span> Reject</button></div>
  </div> : error ? <p role="alert" className="change-error changes-panel-error">{error}</p> : null}
  {history.length > 0 && <section className="change-history" aria-label="Session change history"><h2>SESSION HISTORY</h2>{history.map(entry => <details className="history-entry" key={entry.id}>
    <summary><span className={`history-badge history-${entry.outcome}`}>{entry.outcome === 'applied' ? '✓ APPLIED' : '× REJECTED'}</span><span className="history-summary">{entry.proposal.summary}</span><time dateTime={new Date(entry.timestamp).toISOString()}>{new Date(entry.timestamp).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })}</time></summary>
    <div className="history-detail"><p>{entry.proposal.changes.map(change => change.path).join(', ')}</p><ProposalDiff proposal={entry.proposal} /></div>
  </details>)}</section>}
  {!proposal && history.length === 0 && !error && <div className="changes-empty"><div className="changes-empty-motif"><Elma state="success" size="compact" /><span className="changes-empty-icon" aria-hidden="true">✦</span></div><h2>No proposal changes</h2><p>Edits Elma prepares appear here. Git changes are tracked separately below.</p></div>}</section>
  {projectPath && hasRepository ? <GitPanel projectPath={projectPath} onStatus={onGitStatus} /> : <section className="git-panel"><h2>GIT WORKTREE</h2><p className="git-muted">Open a Git repository to inspect local changes.</p></section>}</aside>
}
