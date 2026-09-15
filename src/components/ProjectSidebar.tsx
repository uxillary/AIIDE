import type { ProjectInfo } from '../types/project'
import { FileTree } from './FileTree'

export function ProjectSidebar({ project, onOpen, busy }: { project: ProjectInfo | null; onOpen: () => void; busy: boolean }) {
  return <aside className="sidebar">
    <div className="sidebar-heading"><span>PROJECT</span><button className="small-button" onClick={onOpen} disabled={busy} title="Open another project"><span aria-hidden="true">▣</span> Open folder</button></div>
    {project ? <>
      <div className="project-card">
        <h2 title={project.name}>{project.name}</h2>
        <p title={project.path}>{project.path}</p>
        <div className="project-meta"><div><span>Repository</span><strong title={project.repository?.name}>{project.repository?.name ?? 'No Git repository'}</strong></div>{project.repository && <><div><span>Branch</span><strong title={project.repository.branch}>{project.repository.branch}</strong></div><div><span>Status</span><strong className={project.repository.changedFiles ? 'status-warn' : 'status-clean'}>{project.repository.changedFiles ? `${project.repository.changedFiles} changed ${project.repository.changedFiles === 1 ? 'file' : 'files'}` : 'Clean'}</strong></div></>}</div>
      </div>
      <div className="files-heading">FILES</div>
      <FileTree entries={project.tree} truncated={project.treeTruncated} />
    </> : <div className="sidebar-empty"><span className="empty-icon" aria-hidden="true">▣</span><p>Your files and Git status will appear here.</p></div>}
  </aside>
}
