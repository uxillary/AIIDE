import { useEffect, useState } from 'react'
import type { ImageEngineStatus } from '../types/image'

function heading(status: ImageEngineStatus) {
  switch (status.state) {
    case 'unavailable': return 'Connect a local ComfyUI service'
    case 'incompatible': return 'This ComfyUI API is not compatible'
    case 'missing_nodes': return 'Required ComfyUI nodes are missing'
    case 'missing_checkpoint': return 'The SDXL checkpoint is missing'
    case 'busy': return 'ComfyUI is busy'
    default: return status.hardwareStatus === 'sufficient' ? 'Image generation is ready' : 'Check the hardware guidance'
  }
}

export function ImageSetupCard({ status, checking, saving, onRetry, onConnect, onDismiss }: {
  status: ImageEngineStatus
  checking: boolean
  saving: boolean
  onRetry: () => void
  onConnect: (endpoint: string, modelId: string) => Promise<void>
  onDismiss: () => void
}) {
  const [endpoint, setEndpoint] = useState(status.endpoint)

  useEffect(() => setEndpoint(status.endpoint), [status.endpoint])

  return <section className="image-setup-card" aria-labelledby="image-setup-heading">
    <span className="eyebrow">IMAGE SETUP · EXTERNAL ENGINE</span>
    <h2 id="image-setup-heading">{heading(status)}</h2>
    <p>{status.error ?? 'The engine and selected model passed the readiness checks. A real GPU generation is still required for acceptance.'}</p>

    {status.missingFiles.length > 0 && <div className="image-requirements"><strong>Required model file</strong><ul>{status.missingFiles.map(file => <li key={file}><code>{file}</code></li>)}</ul><small>Place this user-provided checkpoint in ComfyUI’s checkpoints folder, then retry. AIIDE does not download models in this stage.</small></div>}
    {status.missingNodes.length > 0 && <div className="image-requirements"><strong>Required core nodes</strong><ul>{status.missingNodes.map(node => <li key={node}><code>{node}</code></li>)}</ul></div>}
    {status.hardwareMessage && <p className={`image-hardware image-hardware-${status.hardwareStatus}`}>{status.hardwareMessage}</p>}

    <div className="image-connection-form">
      <label htmlFor="comfyui-endpoint">Existing ComfyUI endpoint</label>
      <div>
        <input id="comfyui-endpoint" type="url" inputMode="url" value={endpoint} onChange={event => setEndpoint(event.target.value)} disabled={saving} />
        <button className="primary-button" onClick={() => void onConnect(endpoint, status.model.id)} disabled={saving || !endpoint.trim()}>{saving ? 'Saving…' : 'Connect'}</button>
      </div>
      <small>Loopback HTTP only. AIIDE remembers this connection but never starts, stops, modifies, or uninstalls the external engine.</small>
    </div>

    <div className="image-setup-model"><span>Selected model</span><strong>{status.model.displayName}</strong><small>{status.model.architecture} · {status.model.workflowId}</small></div>
    <div className="image-setup-actions"><button className="small-button" onClick={onRetry} disabled={checking || saving}>{checking ? 'Checking…' : 'Retry'}</button><button className="subtle-button" onClick={onDismiss}>Skip for now</button></div>
  </section>
}
