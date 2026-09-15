import type { PendingChange, PendingProposal } from '../types/ai'
import { Elma } from './Elma'

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

export function ChangesPanel({ proposal, busy, error, onApply, onReject }: { proposal: PendingProposal | null; busy: boolean; error: string | null; onApply: () => void; onReject: () => void }) {
  return <aside className="changes-panel" aria-label="Changes"><div className="main-label">CHANGES</div>{proposal ? <div className="changes-review">
    <div className="pending-badge">READY FOR REVIEW · NOT APPLIED</div><p className="change-summary">{proposal.summary}</p>
    {proposal.changes.map(change => <section className="change-section" key={change.path}><div className="change-file"><span aria-hidden="true">⌘</span> {change.path}</div><pre className="change-diff">{diff(change).map((line, index) => <div key={index} className={`diff-${line.kind}`}><span>{line.kind === 'delete' ? '-' : line.kind === 'add' ? '+' : ' '}</span>{line.text}</div>)}</pre></section>)}
    {error && <p role="alert" className="change-error">{error}</p>}
    <div className="change-actions"><button className="primary-button" disabled={busy} onClick={onApply}><span aria-hidden="true">✓</span> Apply</button><button className="reject-button" disabled={busy} onClick={onReject}><span aria-hidden="true">×</span> Reject</button></div>
  </div> : <div className="changes-empty"><div className="changes-empty-motif"><Elma state="success" size="compact" /><span className="changes-empty-icon" aria-hidden="true">✦</span></div><h2>No changes yet</h2><p>Edits Elma prepares for you will appear here before anything is written.</p></div>}</aside>
}
