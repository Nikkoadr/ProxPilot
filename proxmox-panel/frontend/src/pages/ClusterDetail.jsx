import { useState, useEffect, useRef, useContext } from 'react'
import { useParams, useNavigate } from 'react-router-dom'
import { ClusterContext } from '../context/ClusterContext'

export default function ClusterDetail() {
  const { id } = useParams()
  const navigate = useNavigate()
  const { refresh } = useContext(ClusterContext)
  const [cluster, setCluster] = useState(null)
  const [logs, setLogs] = useState([])
  const [ws, setWs] = useState(null)
  const logEndRef = useRef(null)

  const poll = async () => {
    try {
      const res = await fetch(`/api/clusters/${id}/status`)
      const data = await res.json()
      setCluster(data)
      setLogs(data.logs || [])
    } catch (e) { console.error(e) }
  }

  useEffect(() => {
    poll()
    const interval = setInterval(poll, 3000)
    return () => clearInterval(interval)
  }, [id])

  useEffect(() => {
    const wsUrl = `ws://${window.location.host}/api/ws/logs/${id}`
    const socket = new WebSocket(wsUrl)
    socket.onopen = () => {
      socket.send(JSON.stringify({ action: 'subscribe' }))
    }
    socket.onmessage = (event) => {
      const data = JSON.parse(event.data)
      if (data.type === 'logs') {
        setLogs(data.logs || [])
        setCluster(prev => prev ? { ...prev, status: data.status, progress: data.progress } : null)
      }
    }
    socket.onclose = () => setTimeout(() => setWs(null), 2000)
    setWs(socket)
    return () => socket.close()
  }, [id])

  useEffect(() => {
    logEndRef.current?.scrollIntoView({ behavior: 'smooth' })
  }, [logs])

  if (!cluster) {
    return (
      <div className="empty-state">
        <div className="icon">⏳</div>
        <h3>Loading cluster details...</h3>
      </div>
    )
  }

  const statusClass = {
    pending: 'badge-pending',
    provisioning: 'badge-provisioning',
    deploying: 'badge-provisioning',
    running: 'badge-running',
    error: 'badge-error',
  }[cluster.status] || 'badge-pending'

  const handleDelete = async () => {
    if (!confirm('Are you sure you want to delete this cluster?')) return
    await fetch(`/api/clusters/${id}`, { method: 'DELETE' })
    refresh()
    navigate('/')
  }

  const handleDeploy = async () => {
    await fetch(`/api/clusters/${id}/deploy`, { method: 'POST' })
  }

  return (
    <div>
      <div className="page-header" style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'flex-start' }}>
        <div>
          <h1>{cluster.cluster?.name || cluster.name}</h1>
          <p style={{ color: 'var(--text-secondary)' }}>
            {cluster.cluster?.master_count || 1} master · {cluster.cluster?.worker_count || 0} worker · Created {new Date(cluster.cluster?.created_at || Date.now()).toLocaleDateString()}
          </p>
        </div>
        <div style={{ display: 'flex', gap: '8px' }}>
          {(cluster.status === 'pending' || cluster.status === 'error') && (
            <button className="btn btn-success" onClick={handleDeploy}>
              🚀 Deploy
            </button>
          )}
          <button className="btn btn-ghost" onClick={() => navigate('/')}>← Back</button>
          <button className="btn btn-danger" onClick={handleDelete}>🗑 Delete</button>
        </div>
      </div>

      {/* Progress */}
      {(cluster.status === 'provisioning' || cluster.status === 'deploying') && (
        <div className="card">
          <div style={{ display: 'flex', justifyContent: 'space-between', marginBottom: '8px' }}>
            <span style={{ fontSize: '0.9rem' }}>Deployment in progress...</span>
            <span style={{ fontSize: '0.9rem', color: 'var(--accent)' }}>{cluster.progress}%</span>
          </div>
          <div className="progress-bar">
            <div className="progress-fill" style={{ width: `${cluster.progress}%` }}></div>
          </div>
        </div>
      )}

      {/* Status */}
      <div className="stats-grid">
        <div className="stat-card">
          <div className="stat-value">
            <span className={`badge ${statusClass}`} style={{ fontSize: '1rem' }}>
              <span className="badge-dot"></span>
              {cluster.status}
            </span>
          </div>
          <div className="stat-label">Cluster Status</div>
        </div>
        <div className="stat-card">
          <div className="stat-value">{cluster.cluster?.master_count || 1}</div>
          <div className="stat-label">Master Nodes</div>
        </div>
        <div className="stat-card">
          <div className="stat-value">{cluster.cluster?.worker_count || 0}</div>
          <div className="stat-label">Worker Nodes</div>
        </div>
        <div className="stat-card">
          <div className="stat-value">{cluster.cluster?.master_cpu}C / {cluster.cluster?.master_ram / 1024}GB</div>
          <div className="stat-label">Master Spec</div>
        </div>
        <div className="stat-card">
          <div className="stat-value">{cluster.cluster?.worker_cpu}C / {cluster.cluster?.worker_ram / 1024}GB</div>
          <div className="stat-label">Worker Spec</div>
        </div>
      </div>

      {/* Logs */}
      <div className="card">
        <div className="card-header">
          <div className="card-title">Deployment Logs</div>
          <span className={`badge ${statusClass}`}><span className="badge-dot"></span>{cluster.status}</span>
        </div>
        <div className="log-console">
          {logs.length === 0 ? (
            <div style={{ color: 'var(--text-secondary)' }}>No logs yet. Click Deploy to start.</div>
          ) : (
            logs.map(log => (
              <div key={log.id} className="log-line">
                <span className="log-time">{new Date(log.timestamp).toLocaleTimeString()}</span>
                <span className={`log-level log-level-${log.level}`}>[{log.phase}]</span>
                <span className="log-message">{log.line}</span>
              </div>
            ))
          )}
          <div ref={logEndRef} />
        </div>
      </div>
    </div>
  )
}
