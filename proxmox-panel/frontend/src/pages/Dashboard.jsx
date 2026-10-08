import { useContext } from 'react'
import { ClusterContext } from '../context/ClusterContext'

export default function Dashboard() {
  const { clusters, nodes, templates, config, loading } = useContext(ClusterContext)

  const running = clusters.filter(c => c.status === 'running').length
  const deploying = clusters.filter(c => c.status === 'provisioning' || c.status === 'deploying').length
  const totalMaster = clusters.reduce((sum, c) => sum + c.master_count, 0)
  const totalWorker = clusters.reduce((sum, c) => sum + c.worker_count, 0)

  if (loading) {
    return (
      <div className="empty-state">
        <div className="icon">⏳</div>
        <h3>Loading...</h3>
      </div>
    )
  }

  return (
    <div>
      <div className="page-header">
        <h1>Dashboard</h1>
        <p>Manage your Kubernetes clusters on Proxmox</p>
      </div>

      <div className="stats-grid">
        <div className="stat-card">
          <div className="stat-value" style={{ color: 'var(--success)' }}>{running}</div>
          <div className="stat-label">Running Clusters</div>
        </div>
        <div className="stat-card">
          <div className="stat-value" style={{ color: 'var(--accent)' }}>{deploying}</div>
          <div className="stat-label">Deploying</div>
        </div>
        <div className="stat-card">
          <div className="stat-value">{totalMaster}</div>
          <div className="stat-label">Master Nodes</div>
        </div>
        <div className="stat-card">
          <div className="stat-value">{totalWorker}</div>
          <div className="stat-label">Worker Nodes</div>
        </div>
        <div className="stat-card">
          <div className="stat-value">{nodes.length}</div>
          <div className="stat-label">Proxmox Nodes</div>
        </div>
        <div className="stat-card">
          <div className="stat-value">{templates.length}</div>
          <div className="stat-label">Available Templates</div>
        </div>
      </div>

      {nodes.length > 0 && (
        <div className="card">
          <div className="card-header">
            <div>
              <div className="card-title">Proxmox Nodes</div>
              <div className="card-subtitle">Connected host infrastructure</div>
            </div>
          </div>
          <div className="stats-grid">
            {nodes.map(node => (
              <div key={node.name} className="stat-card" style={{ padding: '16px' }}>
                <div style={{ display: 'flex', alignItems: 'center', gap: '8px', marginBottom: '12px' }}>
                  <span className="badge badge-running"><span className="badge-dot"></span>{node.status}</span>
                  <strong>{node.name}</strong>
                </div>
                <div style={{ fontSize: '0.8rem', color: 'var(--text-secondary)', display: 'flex', flexDirection: 'column', gap: '4px' }}>
                  <div>CPU: {(node.cpu * 100).toFixed(1)}%</div>
                  <div>Memory: {Math.round(node.used_mem / 1024 / 1024)} / {Math.round(node.memory / 1024 / 1024)} MB</div>
                  <div>Disk: {Math.round(node.used_disk / 1024 / 1024 / 1024)} / {Math.round(node.disk / 1024 / 1024 / 1024)} GB</div>
                </div>
              </div>
            ))}
          </div>
        </div>
      )}

      <div className="card">
        <div className="card-header">
          <div>
            <div className="card-title">Your Clusters</div>
            <div className="card-subtitle">{clusters.length} total clusters</div>
          </div>
        </div>

        {clusters.length === 0 ? (
          <div className="empty-state">
            <div className="icon">🚀</div>
            <h3>No clusters yet</h3>
            <p>Create your first Kubernetes cluster on Proxmox</p>
          </div>
        ) : (
          <div className="cluster-list">
            {clusters.map(cluster => (
              <ClusterCard key={cluster.id} cluster={cluster} />
            ))}
          </div>
        )}
      </div>
    </div>
  )
}

function ClusterCard({ cluster }) {
  const statusMap = {
    pending: 'badge-pending',
    provisioning: 'badge-provisioning',
    deploying: 'badge-provisioning',
    running: 'badge-running',
    error: 'badge-error',
  }

  return (
    <div className="cluster-card">
      <div className="cluster-info">
        <h3>{cluster.name}</h3>
        <div className="cluster-meta">
          <span>🖥 {cluster.master_count} master</span>
          <span>⚙ {cluster.worker_count} worker</span>
          <span>💾 {cluster.master_ram / 1024}GB/{cluster.worker_ram / 1024}GB</span>
          <span>📅 {new Date(cluster.created_at).toLocaleDateString()}</span>
        </div>
      </div>
      <div style={{ display: 'flex', alignItems: 'center', gap: '12px' }}>
        <span className={`badge ${statusMap[cluster.status] || 'badge-pending'}`}>
          <span className="badge-dot"></span>
          {cluster.status}
        </span>
      </div>
    </div>
  )
}
