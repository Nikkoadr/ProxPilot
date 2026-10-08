import { useState, useContext, useEffect } from 'react'
import { useNavigate } from 'react-router-dom'
import { ClusterContext } from '../context/ClusterContext'

const FEATURES = [
  { id: 'k8s', label: 'Kubernetes', icon: '☸', desc: 'K8s master + worker nodes' },
  { id: 'nginx', label: 'Nginx Landing', icon: '🌐', desc: 'Web dashboard & health check' },
  { id: 'monitoring', label: 'Monitoring', icon: '📊', desc: 'Node exporter + metrics' },
]

export default function NewCluster() {
  const navigate = useNavigate()
  const { templates, config } = useContext(ClusterContext)
  const [submitting, setSubmitting] = useState(false)
  const [errors, setErrors] = useState({})

  const [form, setForm] = useState({
    name: '',
    proxmox_url: 'https://192.168.1.100:8006/api2/json',
    proxmox_user: 'root@pam',
    password: '',
    target_node: 'pve',
    clone_template: '',
    network_bridge: 'vmbr0',
    gateway: '192.168.1.1',
    dns1: '8.8.8.8',
    master_count: 1,
    master_cpu: 4,
    master_ram: 8192,
    worker_count: 2,
    worker_cpu: 2,
    worker_ram: 4096,
    ssh_user: 'ubuntu',
    ssh_public_key: '',
    enabled_features: ['k8s', 'nginx'],
  })

  const handleChange = (field, value) => {
    setForm(prev => ({ ...prev, [field]: value }))
    if (errors[field]) setErrors(prev => ({ ...prev, [field]: '' }))
  }

  const toggleFeature = (featureId) => {
    const features = form.enabled_features.includes(featureId)
      ? form.enabled_features.filter(f => f !== featureId)
      : [...form.enabled_features, featureId]
    setForm(prev => ({ ...prev, enabled_features: features }))
  }

  const validate = () => {
    const errs = {}
    if (!form.name.trim()) errs.name = 'Cluster name is required'
    if (!form.password.trim()) errs.password = 'Password is required'
    if (!form.clone_template) errs.clone_template = 'Select a template'
    if (form.master_count < 1) errs.master_count = 'At least 1 master'
    if (form.worker_count < 0) errs.worker_count = 'Invalid worker count'
    if (form.master_cpu < 1 || form.master_cpu > 32) errs.master_cpu = '1-32 cores'
    if (form.master_ram < 1024 || form.master_ram > 65536) errs.master_ram = '1024-65536 MB'
    if (form.worker_cpu < 1 || form.worker_cpu > 16) errs.worker_cpu = '1-16 cores'
    if (form.worker_ram < 512 || form.worker_ram > 32768) errs.worker_ram = '512-32768 MB'
    setErrors(errs)
    return Object.keys(errs).length === 0
  }

  const handleSubmit = async (e) => {
    e.preventDefault()
    if (!validate()) return

    setSubmitting(true)
    try {
      const res = await fetch('/api/clusters', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(form),
      })
      const cluster = await res.json()
      navigate(`/cluster/${cluster.id}`)
    } catch (err) {
      setErrors({ submit: err.message })
    } finally {
      setSubmitting(false)
    }
  }

  return (
    <div>
      <div className="page-header">
        <h1>Create Cluster</h1>
        <p>Configure and deploy your Kubernetes cluster on Proxmox</p>
      </div>

      <form onSubmit={handleSubmit}>
        {/* Proxmox Connection */}
        <div className="card">
          <div className="card-header">
            <div>
              <div className="card-title">Proxmox Connection</div>
              <div className="card-subtitle">Connect to your Proxmox VE cluster</div>
            </div>
          </div>
          <div className="form-grid">
            <div className="form-group">
              <label className="form-label">Cluster Name *</label>
              <input className="form-input" placeholder="my-k8s-cluster"
                value={form.name} onChange={e => handleChange('name', e.target.value)} />
              {errors.name && <span className="form-hint" style={{ color: 'var(--danger)' }}>{errors.name}</span>}
            </div>
            <div className="form-group">
              <label className="form-label">Proxmox URL *</label>
              <input className="form-input" placeholder="https://192.168.1.100:8006/api2/json"
                value={form.proxmox_url} onChange={e => handleChange('proxmox_url', e.target.value)} />
            </div>
            <div className="form-group">
              <label className="form-label">Username</label>
              <input className="form-input" placeholder="root@pam"
                value={form.proxmox_user} onChange={e => handleChange('proxmox_user', e.target.value)} />
            </div>
            <div className="form-group">
              <label className="form-label">Password *</label>
              <input className="form-input" type="password" placeholder="Proxmox password"
                value={form.password} onChange={e => handleChange('password', e.target.value)} />
              {errors.password && <span className="form-hint" style={{ color: 'var(--danger)' }}>{errors.password}</span>}
            </div>
            <div className="form-group">
              <label className="form-label">Target Node</label>
              <input className="form-input" placeholder="pve"
                value={form.target_node} onChange={e => handleChange('target_node', e.target.value)} />
            </div>
            <div className="form-group">
              <label className="form-label">VM Template *</label>
              <select className="form-select"
                value={form.clone_template} onChange={e => handleChange('clone_template', e.target.value)}>
                <option value="">Select template...</option>
                {templates.map(t => (
                  <option key={t.name} value={t.name}>{t.name} ({t.size})</option>
                ))}
              </select>
              {errors.clone_template && <span className="form-hint" style={{ color: 'var(--danger)' }}>{errors.clone_template}</span>}
            </div>
          </div>
        </div>

        {/* Network */}
        <div className="card">
          <div className="card-header">
            <div>
              <div className="card-title">Network Configuration</div>
              <div className="card-subtitle">VM network settings</div>
            </div>
          </div>
          <div className="form-grid">
            <div className="form-group">
              <label className="form-label">Network Bridge</label>
              <input className="form-input" placeholder="vmbr0"
                value={form.network_bridge} onChange={e => handleChange('network_bridge', e.target.value)} />
            </div>
            <div className="form-group">
              <label className="form-label">Gateway</label>
              <input className="form-input" placeholder="192.168.1.1"
                value={form.gateway} onChange={e => handleChange('gateway', e.target.value)} />
            </div>
            <div className="form-group">
              <label className="form-label">DNS Server</label>
              <input className="form-input" placeholder="8.8.8.8"
                value={form.dns1} onChange={e => handleChange('dns1', e.target.value)} />
            </div>
            <div className="form-group">
              <label className="form-label">SSH User</label>
              <input className="form-input" placeholder="ubuntu"
                value={form.ssh_user} onChange={e => handleChange('ssh_user', e.target.value)} />
            </div>
          </div>
        </div>

        {/* Master Node */}
        <div className="card">
          <div className="card-header">
            <div>
              <div className="card-title">Master Node(s)</div>
              <div className="card-subtitle">Kubernetes control plane</div>
            </div>
          </div>
          <div className="form-grid">
            <div className="form-group">
              <label className="form-label">Master Count</label>
              <input className="form-input" type="number" min="1" max="3"
                value={form.master_count} onChange={e => handleChange('master_count', parseInt(e.target.value) || 1)} />
              {errors.master_count && <span className="form-hint" style={{ color: 'var(--danger)' }}>{errors.master_count}</span>}
            </div>
            <div className="form-group">
              <label className="form-label">CPU Cores</label>
              <input className="form-input" type="number" min="1" max="32"
                value={form.master_cpu} onChange={e => handleChange('master_cpu', parseInt(e.target.value) || 4)} />
              {errors.master_cpu && <span className="form-hint" style={{ color: 'var(--danger)' }}>{errors.master_cpu}</span>}
            </div>
            <div className="form-group">
              <label className="form-label">RAM (MB)</label>
              <input className="form-input" type="number" step="512" min="1024" max="65536"
                value={form.master_ram} onChange={e => handleChange('master_ram', parseInt(e.target.value) || 8192)} />
              {errors.master_ram && <span className="form-hint" style={{ color: 'var(--danger)' }}>{errors.master_ram}</span>}
            </div>
          </div>
        </div>

        {/* Worker Node */}
        <div className="card">
          <div className="card-header">
            <div>
              <div className="card-title">Worker Node(s)</div>
              <div className="card-subtitle">Kubernetes worker nodes</div>
            </div>
          </div>
          <div className="form-grid">
            <div className="form-group">
              <label className="form-label">Worker Count</label>
              <input className="form-input" type="number" min="0" max="10"
                value={form.worker_count} onChange={e => handleChange('worker_count', parseInt(e.target.value) || 2)} />
              {errors.worker_count && <span className="form-hint" style={{ color: 'var(--danger)' }}>{errors.worker_count}</span>}
            </div>
            <div className="form-group">
              <label className="form-label">CPU Cores</label>
              <input className="form-input" type="number" min="1" max="16"
                value={form.worker_cpu} onChange={e => handleChange('worker_cpu', parseInt(e.target.value) || 2)} />
              {errors.worker_cpu && <span className="form-hint" style={{ color: 'var(--danger)' }}>{errors.worker_cpu}</span>}
            </div>
            <div className="form-group">
              <label className="form-label">RAM (MB)</label>
              <input className="form-input" type="number" step="512" min="512" max="32768"
                value={form.worker_ram} onChange={e => handleChange('worker_ram', parseInt(e.target.value) || 4096)} />
              {errors.worker_ram && <span className="form-hint" style={{ color: 'var(--danger)' }}>{errors.worker_ram}</span>}
            </div>
          </div>
        </div>

        {/* Features */}
        <div className="card">
          <div className="card-header">
            <div>
              <div className="card-title">Post-Deploy Features</div>
              <div className="card-subtitle">Optional services to install after cluster is ready</div>
            </div>
          </div>
          <div className="toggle-group">
            {FEATURES.map(f => (
              <div key={f.id} className={`toggle-chip ${form.enabled_features.includes(f.id) ? 'active' : ''}`}
                onClick={() => toggleFeature(f.id)}>
                <span>{f.icon}</span> {f.label}
              </div>
            ))}
          </div>
        </div>

        {errors.submit && (
          <div style={{ color: 'var(--danger)', padding: '12px', marginBottom: '16px' }}>{errors.submit}</div>
        )}

        <div style={{ display: 'flex', gap: '12px', justifyContent: 'flex-end' }}>
          <button type="button" className="btn btn-ghost" onClick={() => navigate('/')}>Cancel</button>
          <button type="submit" className="btn btn-primary btn-lg" disabled={submitting}>
            {submitting ? '⏳ Creating...' : '🚀 Create & Deploy'}
          </button>
        </div>
      </form>
    </div>
  )
}
