let allVms = [], templates = [], selectedVms = new Set(), selectedTemplate = null, currentRunId = null;
let pollTimer = null;

const TEMPLATE_DEFS = [
  { id: 'k8s-master', icon: 'fa-cubes', color: 'primary', desc: 'Kubernetes Master (init + Calico)', note: 'Inisialisasi cluster k8s, instal kubelet/kubeadm/kubectl' },
  { id: 'k8s-worker', icon: 'fa-cube', color: 'info', desc: 'Kubernetes Worker (join)', note: 'Gabung ke cluster k8s yang sudah ada' },
  { id: 'redis', icon: 'fa-database', color: 'danger', desc: 'Redis Server', note: 'Instal & konfigurasikan Redis sebagai cache/session store' },
  { id: 'postgres', icon: 'fa-server', color: 'warning', desc: 'PostgreSQL Server', note: 'Instal & konfigurasikan PostgreSQL database' },
  { id: 'nodejs', icon: 'fa-node', color: 'success', desc: 'Node.js LTS + PM2', note: 'Runtime Node.js LTS, npm, pm2 process manager' },
  { id: 'nginx', icon: 'fa-globe', color: 'secondary', desc: 'Nginx Landing Page', note: 'Web server Nginx dengan default landing page' },
];

async function loadTemplates() {
  try {
    const r = await apiGet('/api/configure/templates');
    templates = r || [];
    renderTemplates();
  } catch (e) {
    renderTemplates();
  }
}

function renderTemplates() {
  const container = document.getElementById('templateRows');
  if (!container) return;
  if (!templates.length) {
    container.innerHTML = '<div class="col-12 text-center text-muted">Tidak ada template tersedia.</div>';
    return;
  }
  container.innerHTML = templates.map(t => {
    const def = TEMPLATE_DEFS.find(d => d.id === t.id) || { icon: 'fa-cog', color: 'secondary', desc: t.description || t.id, note: '' };
    const sel = selectedTemplate === t.id ? 'border-primary shadow' : '';
    return `<div class="col-md-4 mb-3">
      <div class="card h-100 ${sel} cursor-pointer" onclick="selectTemplate('${esc(t.id)}')" style="cursor:pointer">
        <div class="card-body">
          <div class="d-flex align-items-center mb-2">
            <div class="icon-circle bg-${def.color} text-white mr-3">
              <i class="fas ${def.icon}"></i>
            </div>
            <div>
              <h6 class="m-0 font-weight-bold">${esc(def.desc)}</h6>
              <small class="text-muted">${esc(t.playbook || '')}</small>
            </div>
          </div>
          <p class="small text-muted mb-0">${esc(def.note)}</p>
          ${selectedTemplate === t.id ? '<div class="mt-2"><span class="badge badge-primary"><i class="fas fa-check"></i> Dipilih</span></div>' : ''}
        </div>
      </div>
    </div>`;
  }).join('');
}

function selectTemplate(id) {
  selectedTemplate = selectedTemplate === id ? null : id;
  renderTemplates();
  updateRunButton();
}

async function loadVms() {
  try {
    const r = await apiGet('/api/vms');
    allVms = (r && r.vms) || [];
    renderVmTable();
  } catch (e) {
    const tb = document.getElementById('vmCheckRows');
    if (tb) tb.innerHTML = '<tr><td colspan="7" class="text-center text-muted">Gagal memuat VM: ' + esc(String(e)) + '</td></tr>';
  }
}

function renderVmTable() {
  const tb = document.getElementById('vmCheckRows');
  const footer = document.getElementById('vmCheckFooter');
  if (!tb) return;
  const running = allVms.filter(v => !v.template);
  if (!running.length) {
    tb.innerHTML = '<tr><td colspan="7" class="text-center text-muted">Tidak ada VM. Clone dulu di halaman <a href="/clone-vm.html">Clone VM</a>.</td></tr>';
    if (footer) footer.style.display = 'none';
    return;
  }
  if (footer) {
    footer.style.display = '';
    footer.textContent = `Total ${running.length} VM. Klik checkbox untuk memilih target konfigurasi.`;
  }
  tb.innerHTML = running.map(v => {
    const checked = selectedVms.has(v.vmid) ? 'checked' : '';
    const running = (v.status || '').toLowerCase() === 'running';
    const badge = running ? 'badge-success' : 'badge-secondary';
    const cpu = ((v.cpu || 0) * 100).toFixed(1) + '%';
    const mem = fmtMem(v.mem) + ' / ' + fmtMem(v.maxmem);
    const ip = '-';
    return `<tr>
      <td><input type="checkbox" class="vm-chk" data-vmid="${v.vmid}" ${checked}></td>
      <td><strong>${v.vmid}</strong></td>
      <td>${esc(v.name)}</td>
      <td><span class="badge ${badge}">${esc(v.status)}</span></td>
      <td class="small">${cpu}<br>${mem}</td>
      <td class="small">${fmtUptime(v.uptime)}</td>
      <td class="small text-muted">${ip}</td>
    </tr>`;
  }).join('');

  tb.querySelectorAll('.vm-chk').forEach(chk => {
    chk.onchange = () => {
      const vmid = parseInt(chk.dataset.vmid, 10);
      if (chk.checked) selectedVms.add(vmid);
      else selectedVms.delete(vmid);
      updateVmCount();
      // sync "select all"
      const allChk = document.getElementById('chkAll');
      if (allChk) allChk.checked = selectedVms.size === running.length && running.length > 0;
    };
  });

  const allChk = document.getElementById('chkAll');
  if (allChk) {
    allChk.onchange = () => {
      if (allChk.checked) {
        running.forEach(v => selectedVms.add(v.vmid));
      } else {
        selectedVms.clear();
      }
      renderVmTable();
      updateVmCount();
    };
  }
  updateVmCount();
}

function updateVmCount() {
  const el = document.getElementById('vmCountLabel');
  if (el) el.textContent = selectedVms.size + ' dipilih';
}

function updateRunButton() {
  const btn = document.getElementById('btnRun');
  if (!btn) return;
  const canRun = selectedVms.size > 0 && selectedTemplate !== null;
  btn.disabled = !canRun;
  btn.innerHTML = canRun
    ? `<i class="fas fa-play"></i> Jalankan (${selectedVms.size} VM × ${selectedTemplate})`
    : '<i class="fas fa-play"></i> Jalankan Ansible';
}

async function runConfigure() {
  const btn = document.getElementById('btnRun');
  if (selectedVms.size === 0 || !selectedTemplate) return;
  btn.disabled = true;
  btn.textContent = 'Menjalankan...';

  const sshUser = document.getElementById('sshUser').value;
  const body = {
    vmids: Array.from(selectedVms),
    template: selectedTemplate,
    ssh_user: sshUser,
  };

  try {
    const r = await apiPost('/api/configure', body);
    const runId = r.run_id;
    showResult(true, 'Ansible dijalankan!', runId);
    pollRun(runId);
  } catch (e) {
    showResult(false, 'Gagal', String(e.message || e));
    btn.disabled = false;
    btn.innerHTML = '<i class="fas fa-play"></i> Jalankan Ansible';
  }
}

function showResult(ok, title, bodyHtml) {
  const card = document.getElementById('resultCard');
  const t = document.getElementById('resultTitle');
  const o = document.getElementById('resultOutput');
  const a = document.getElementById('resultActions');
  t.textContent = title;
  t.className = 'm-0 font-weight-bold ' + (ok ? 'text-success' : 'text-danger');
  o.textContent = typeof bodyHtml === 'string' ? bodyHtml : JSON.stringify(bodyHtml, null, 2);
  o.style.borderLeft = ok ? '4px solid #1cc88a' : '4px solid #e74a3b';
  a.innerHTML = ok
    ? '<span class="small text-muted">Polling status... refresh halaman setelah selesai.</span>'
    : '<button class="btn btn-sm btn-secondary" onclick="showResult(false,\'' + title + '\',document.getElementById(\'resultOutput\').textContent)">Tutup</button>';
  card.style.display = '';
}

async function pollRun(runId) {
  if (pollTimer) clearInterval(pollTimer);
  currentRunId = runId;
  const poll = async () => {
    try {
      const r = await apiGet('/api/configure/runs/' + encodeURIComponent(runId));
      const o = document.getElementById('resultOutput');
      const a = document.getElementById('resultActions');
      if (o) o.textContent = r.output || '(belum ada output)';
      if (a) {
        if (r.status === 'done') {
          a.innerHTML = '<span class="badge badge-success"><i class="fas fa-check"></i> Selesai</span> ' +
            '<button class="btn btn-sm btn-outline-secondary ml-2" onclick="document.getElementById(\'resultCard\').style.display=\'none\'">Tutup</button>';
          if (pollTimer) clearInterval(pollTimer);
        } else if (r.status === 'failed') {
          a.innerHTML = '<span class="badge badge-danger"><i class="fas fa-times"></i> Gagal</span> ' +
            '<button class="btn btn-sm btn-outline-secondary ml-2" onclick="document.getElementById(\'resultCard\').style.display=\'none\'">Tutup</button>';
          if (pollTimer) clearInterval(pollTimer);
        }
      }
    } catch (e) { /* ignore */ }
  };
  poll();
  pollTimer = setInterval(poll, 3000);
}

function fmtUptime(s) {
  s = parseInt(s, 10) || 0;
  if (s < 60) return s + 's';
  const m = Math.floor(s / 60), h = Math.floor(m / 60), d = Math.floor(h / 24);
  if (d) return d + 'd ' + (h % 24) + 'h';
  if (h) return h + 'h ' + (m % 60) + 'm';
  return m + 'm';
}

function fmtMem(mb) {
  if (mb == null) return '-';
  const m = Math.round(mb / 1048576);
  if (m >= 1024) return (m / 1024).toFixed(1) + ' GB';
  return m + ' MB';
}

document.addEventListener('DOMContentLoaded', () => {
  loadTemplates();
  loadVms();
  document.getElementById('btnSelectAll').onclick = () => {
    allVms.filter(v => !v.template).forEach(v => selectedVms.add(v.vmid));
    renderVmTable();
    updateRunButton();
  };
  document.getElementById('btnDeselectAll').onclick = () => {
    selectedVms.clear();
    renderVmTable();
    updateRunButton();
  };
  document.getElementById('btnRun').onclick = runConfigure;
  document.getElementById('sshUser').onchange = updateRunButton;
});
