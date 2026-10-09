let setupData = null, templatesList = [], vmList = [];

function toggleStatic() {
  const on = document.getElementById('cIpMode').value === 'static';
  document.getElementById('cStaticIpWrap').style.display = on ? '' : 'none';
  document.getElementById('cGatewayWrap').style.display = on ? '' : 'none';
}

async function loadSetup() {
  try {
    setupData = await apiGet('/api/setup');
    document.getElementById('snNode').textContent = setupData.target_node || '-';
    const key = await apiGet('/api/ssh/key').catch(() => ({}));
    document.getElementById('snKey').innerHTML = key.exists
      ? '<span class="badge badge-success">ada</span>'
      : '<span class="badge badge-warning">belum</span>';
  } catch (e) {
    document.getElementById('snNode').textContent = 'setup belum';
    document.getElementById('snKey').textContent = 'error';
  }
  await loadTemplates();
  await loadVms();
}

async function loadTemplates() {
  try {
    templatesList = await apiGet('/api/templates');
    const sel = document.getElementById('cTemplate');
    sel.innerHTML = '<option value="">-- Pilih Template --</option>' +
      (templatesList || []).map(t => `<option value="${esc(t.name)}">${esc(t.name)}${t.vmid ? ' (vmid ' + t.vmid + ')' : ''}</option>`).join('');
    document.getElementById('snTemplates').textContent = templatesList.length + ' templates';
  } catch (e) {
    document.getElementById('cTemplate').innerHTML = '<option value="">gagal load</option>';
    document.getElementById('snTemplates').textContent = 'error';
  }
}

async function loadVms() {
  try {
    const r = await apiGet('/api/vms');
    vmList = r.vms || [];
    const live = vmList.filter(v => !v.template);
    document.getElementById('snVms').textContent = live.length + ' VM';
    renderVms(vmList);
  } catch (e) {
    document.getElementById('snVms').textContent = 'error';
    const tb = document.getElementById('vmRows');
    if (tb) tb.innerHTML = '<tr><td colspan="7" class="text-center text-muted">Tidak bisa load VM — pastikan Setup sudah diisi atau buat cluster dengan token API Proxmox.</td></tr>';
  }
}

function renderVms(vms) {
  const tb = document.getElementById('vmRows');
  if (!tb) return;
  const nonTpl = vms.filter(v => !v.template);
  if (!nonTpl.length) {
    tb.innerHTML = '<tr><td colspan="7" class="text-center text-muted">Belum ada VM (hanya template). Clone dulu di atas.</td></tr>';
    return;
  }
  tb.innerHTML = nonTpl.map(v => {
    const running = (v.status || '').toLowerCase() === 'running';
    const badge = running ? 'success' : 'secondary';
    const cpu = ((v.cpu || 0) * 100).toFixed(1) + '% / ' + (v.cpus || '?') + 'c';
    const mem = fmtMem(v.mem) + ' / ' + fmtMem(v.maxmem);
    const up = fmtUptime(v.uptime);
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
      <td><span class="badge badge-${badge}">${esc(v.status)}</span></td>
      <td class="small">${cpu}<br>${mem}</td>
      <td class="small">${up}</td>
      <td>${btns}</td>
    </tr>`;
  }).join('');
}

async function vmAct(vmid, action, confirmIt) {
  if (confirmIt && !confirm(action + ' VM ' + vmid + '?')) return;
  try { await apiPost('/api/vms/' + vmid + '/' + action, {}); }
  catch (e) { alert(String(e.message || e)); }
  loadVms();
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

function showResult(ok, title, bodyHtml) {
  const card = document.getElementById('resultCard');
  const t = document.getElementById('resultTitle');
  const b = document.getElementById('resultBody');
  t.textContent = title;
  t.className = 'm-0 font-weight-bold ' + (ok ? 'text-success' : 'text-danger');
  b.innerHTML = bodyHtml;
  card.style.display = '';
  card.className = 'card shadow mb-4 ' + (ok ? 'border-success' : 'border-danger');
  card.style.borderWidth = ok ? '0 0 2px 0' : '0 0 2px 0';
  card.style.borderStyle = 'solid';
  card.style.borderColor = ok ? '#1cc88a' : '#e74a3b';
}

async function submitClone(e) {
  e.preventDefault();
  const err = document.getElementById('cErr');
  err.textContent = '';
  const btn = document.getElementById('btnClone');
  btn.disabled = true;
  btn.textContent = 'Cloning...';

  const name = document.getElementById('cName').value.trim();
  const template = document.getElementById('cTemplate').value;
  const vmidRaw = document.getElementById('cVmid').value.trim();
  const vmid = vmidRaw ? parseInt(vmidRaw, 10) : undefined;
  const full = document.getElementById('cFull').value === '1';
  const cpu = parseInt(document.getElementById('cCpu').value, 10) || 2;
  const ram = parseInt(document.getElementById('cRam').value, 10) || 4096;
  const disk = parseInt(document.getElementById('cDisk').value, 10) || 0;
  const storage = document.getElementById('cDiskStorage').value.trim() || 'local-lvm';
  const bridge = document.getElementById('cBridge').value.trim() || 'vmbr0';
  const ciuser = document.getElementById('cCiuser').value.trim() || 'ubuntu';
  const ipMode = document.getElementById('cIpMode').value;
  const staticIp = document.getElementById('cStaticIp').value.trim();
  const gateway = document.getElementById('cGateway').value.trim();
  const nameserver = document.getElementById('cDns').value.trim() || '8.8.8.8';
  const start = document.getElementById('cStart').checked;

  if (!template) { err.textContent = 'Pilih template dulu!'; btn.disabled = false; btn.textContent = 'Clone VM'; return; }
  if (!name) { err.textContent = 'VM name wajib diisi!'; btn.disabled = false; btn.textContent = 'Clone VM'; return; }
  const okName = /^[a-zA-Z0-9][a-zA-Z0-9\-]{0,62}[a-zA-Z0-9]$/.test(name);
  if (!okName && name.length <= 63) { err.textContent = 'Nama VM: hanya huruf, angka, strip. Tidak boleh diawali/diakhiri strip.'; btn.disabled = false; btn.textContent = 'Clone VM'; return; }

  const body = {
    template, name,
    vmid: vmid,
    full,
    storage,
    static_ip: ipMode === 'static' ? staticIp : '',
    gateway: gateway || (ipMode === 'static' ? '' : ''),
    ciuser,
    nameserver,
    start,
  };

  try {
    const r = await apiPost('/api/vms/clone', body);
    let html = '<div class="alert alert-success"><strong>VM berhasil di-clone!</strong><br>';
    html += 'VMID: <strong>' + r.vmid + '</strong> | Nama: <strong>' + esc(r.name) + '</strong>';
    if (r.ip) html += '<br>IP: <strong class="text-success">' + esc(r.ip) + '</strong>';
    else html += '<br><span class="text-muted small">IP belum terbaca ( DHCP / agent lambat ) — cek beberapa saat lagi</span>';
    if (r.hint) html += '<br><span class="small text-muted">' + esc(r.hint) + '</span>';
    html += '</div>';
    html += '<a href="/configure.html" class="btn btn-primary"><i class="fas fa-cog"></i> Lanjut Configure</a> ';
    html += '<button class="btn btn-outline-secondary" onclick="loadVms()">Refresh List</button>';
    showResult(true, 'Clone Berhasil', html);
    document.getElementById('cName').value = '';
    await loadVms();
  } catch (e) {
    showResult(false, 'Clone Gagal', '<div class="alert alert-danger">' + esc(String(e.message || e)) + '</div>');
  }
  btn.disabled = false;
  btn.textContent = 'Clone VM';
}

document.addEventListener('DOMContentLoaded', () => {
  loadSetup();
  document.getElementById('cloneForm').onsubmit = submitClone;
  document.getElementById('btnRefresh').onclick = loadVms;
});
