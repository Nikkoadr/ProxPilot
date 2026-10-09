async function loadDashboard() {
  try {
    const [summary, clusters, nodes, tools, allVms, sshKey] = await Promise.all([
      apiGet('/api/realtime/summary'),
      apiGet('/api/clusters'),
      apiGet('/api/nodes'),
      apiGet('/api/tools'),
      apiGet('/api/vms').catch(() => ({ vms: [] })),
      apiGet('/api/ssh/key').catch(() => ({})),
    ]);

    // Stats
    setText('statVms', summary.master_nodes + summary.worker_nodes);
    setText('statDeploying', summary.deploying);
    setText('statClusters', summary.clusters_total);
    setText('statNodes', Array.isArray(nodes) ? nodes.length : 0);

    // Quick: SSH key
    const ks = document.getElementById('qKeyState');
    if (ks) {
      ks.className = 'badge ' + (sshKey.exists ? 'badge-success' : 'badge-warning');
      ks.textContent = sshKey.exists ? 'ada' : 'belum ada';
    }

    // Quick: tools
    const qt = document.getElementById('qToolsState');
    if (qt && tools) {
      const ok = (tools.terraform?.wsl?.ok || tools.ansible?.wsl?.ok);
      qt.className = 'badge ' + (ok ? 'badge-success' : 'badge-danger');
      qt.textContent = ok ? 'ready' : 'missing';
    }

    // Quick: template count
    const qt2 = document.getElementById('qTemplateCount');
    if (qt2) qt2.textContent = (allVms.templates || []).length + ' templates';

    // Quick: VM ready (running non-template)
    const qr = document.getElementById('qVmReady');
    if (qr) {
      const running = (allVms.vms || []).filter(v => !v.template && v.status === 'running').length;
      qr.className = 'badge ' + (running > 0 ? 'badge-success' : 'badge-secondary');
      qr.textContent = running + ' ready';
    }

    // Workflow stepper
    updateWorkflow(allVms, sshKey);

    // VM table (global, non-template only)
    renderVms(allVms);

    // Cluster table
    renderClusters(clusters);
  } catch (e) {
    console.warn(e);
  }
}

function updateWorkflow(vms, key) {
  const hasKey = key.exists;
  const hasTemplates = (vms.templates || []).length > 0;
  const wf1 = document.getElementById('wf1');
  const wf2 = document.getElementById('wf2');
  const wf3 = document.getElementById('wf3');
  const btnClone = document.getElementById('btnWfClone');
  const btnConfig = document.getElementById('btnWfConfig');

  if (wf1) wf1.className = 'col-md-4 wf-step' + (hasKey ? ' done' : ' active');
  if (wf2) wf2.className = 'col-md-4 wf-step' + (hasKey && hasTemplates ? ' done' : hasKey ? ' active' : '');
  if (wf3) wf3.className = 'col-md-4 wf-step' + (hasKey && hasTemplates ? ' done' : '');

  if (btnClone) {
    btnClone.className = 'btn btn-sm ' + (hasKey && hasTemplates ? 'btn-success' : 'btn-secondary');
  }
  if (btnConfig) {
    const hasRunning = (vms.vms || []).some(v => !v.template && v.status === 'running');
    btnConfig.className = 'btn btn-sm ' + (hasRunning ? 'btn-warning' : 'btn-secondary');
  }
}

function renderVms(data) {
  const tb = document.getElementById('vmRows');
  if (!tb) return;
  const vms = (data && data.vms) || [];
  if (!vms.length) {
    tb.innerHTML = '<tr><td colspan="7" class="text-center text-muted">Belum ada VM. Clone dulu di halaman Clone VM setelah koneksi SSH &amp; API Proxmox tersedia.</td></tr>';
    return;
  }
  tb.innerHTML = vms.map(v => {
    if (v.template) return '';
    const running = (v.status || '').toLowerCase() === 'running';
    const badgeClass = running ? 'badge-live' : 'badge-stop';
    const cpu = ((v.cpu || 0) * 100).toFixed(1) + '% / ' + (v.cpus || '?') + 'c';
    const mem = fmtMem(v.mem) + ' / ' + fmtMem(v.maxmem);
    const uptime = fmtUptime(v.uptime);
    let btns = '';
    if (running) {
      btns = `<button class="btn btn-sm btn-warning" onclick="vmAct(${v.vmid},'reboot',1)" title="Reboot"><i class="fas fa-redo"></i></button>
        <button class="btn btn-sm btn-secondary ml-1" onclick="vmAct(${v.vmid},'shutdown',1)" title="Shutdown"><i class="fas fa-power-off"></i></button>
        <button class="btn btn-sm btn-danger ml-1" onclick="vmAct(${v.vmid},'stop',1)" title="Stop"><i class="fas fa-stop"></i></button>`;
    } else {
      btns = `<button class="btn btn-sm btn-success" onclick="vmAct(${v.vmid},'start',0)" title="Start"><i class="fas fa-play"></i></button>`;
    }
    return `<tr>
      <td><strong>${v.vmid}</strong></td>
      <td>${esc(v.name)}</td>
      <td><span class="badge ${badgeClass}">${esc(v.status)}</span></td>
      <td class="small">${cpu}<br>${mem}</td>
      <td class="small">${uptime}</td>
      <td>${btns}</td>
    </tr>`;
  }).filter(Boolean).join('') || '<tr><td colspan="7" class="text-center text-muted">Semua template, tidak ada VM biasa.</td></tr>';
}

function renderClusters(clusters) {
  const tb = document.getElementById('clusterRows');
  if (!tb) return;
  tb.innerHTML = (clusters || []).length
    ? clusters.map(clusterRow).join('')
    : '<tr><td colspan="5" class="text-center text-muted">Belum ada cluster. Buat di <a href="/new-cluster.html">New Cluster</a>.</td></tr>';
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

async function vmAct(vmid, action, confirmIt) {
  if (confirmIt && !confirm(action + ' VM ' + vmid + '?')) return;
  try { await apiPost('/api/vms/' + vmid + '/' + action, {}); }
  catch (e) { alert(String(e.message || e)); }
  loadDashboard();
}

document.addEventListener('DOMContentLoaded', () => {
  loadDashboard();
  setInterval(loadDashboard, 10000);
  const btn = document.getElementById('btnVmRefresh');
  if (btn) btn.onclick = loadDashboard;
});
