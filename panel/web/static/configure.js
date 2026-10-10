let rows = [], selected = new Set(), selectedTpl = null, srcMode = 'live';
// rows: {key, vmid?, name, ip?, status}
async function loadSources(){
  try {
    const clusters = await apiGet('/api/clusters');
    const sel = document.getElementById('srcSel');
    sel.innerHTML = '<option value="live">Live VMs (via agent)</option>' +
      clusters.map(c => `<option value="cluster:${c.id}">Cluster ${esc(c.name)} (${esc(c.status)})</option>`).join('');
  } catch(e){}
}
async function loadRows(){
  selected.clear();
  const src = document.getElementById('srcSel').value;
  srcMode = src.startsWith('cluster:') ? 'cluster' : 'live';
  if (srcMode === 'cluster') {
    const id = src.slice(8);
    try {
      const hosts = await apiGet('/api/clusters/' + encodeURIComponent(id) + '/hosts');
      rows = hosts.map(h => ({key: 'c:' + h.vm_name, name: h.vm_name, ip: h.ip || '', vmid: h.vmid, status: h.ip ? 'static-ip' : 'no-ip'}));
    } catch(e){ rows = []; toast('error', e.message); }
  } else {
    try {
      const r = await apiGet('/api/vms');
      rows = (r.vms || []).filter(v => !v.template).map(v => ({key: 'v:' + v.vmid, vmid: v.vmid, name: v.name, ip: '', status: v.status, cpu: v.cpu, cpus: v.cpus, mem: v.mem, maxmem: v.maxmem, uptime: v.uptime}));
    } catch(e){ rows = []; toast('error', e.message); }
  }
  renderRows();
}
function renderRows(){
  const tb = document.getElementById('vmRows');
  if (!rows.length) { tb.innerHTML = '<tr><td colspan="6" class="text-center text-muted">Tidak ada host. Clone/Deploy dulu.</td></tr>'; updateCount(); return; }
  tb.innerHTML = rows.map(r => {
    const sub = srcMode === 'cluster' ? esc(r.ip || 'tanpa IP') : (((r.cpu||0)*100).toFixed(1) + '%<br>' + fmtMem(r.mem) + ' / ' + fmtMem(r.maxmem));
    const up = srcMode === 'cluster' ? ('vmid ' + r.vmid) : fmtUptime(r.uptime);
    return `<tr><td><input type="checkbox" class="vm-chk" data-key="${esc(r.key)}" ${selected.has(r.key) ? 'checked' : ''}></td>
      <td><strong>${r.vmid}</strong></td><td>${esc(r.name)}</td>
      <td><span class="badge badge-secondary">${esc(r.status)}</span></td>
      <td class="small">${sub}</td><td class="small">${up}</td></tr>`;
  }).join('');
  tb.querySelectorAll('.vm-chk').forEach(chk => chk.onchange = () => {
    chk.checked ? selected.add(chk.dataset.key) : selected.delete(chk.dataset.key);
    updateCount();
  });
  updateCount();
}
function updateCount(){
  document.getElementById('vmCount').textContent = selected.size + ' dipilih';
  const btn = document.getElementById('btnRun');
  btn.disabled = !(selected.size > 0 && selectedTpl);
  btn.innerHTML = (selected.size > 0 && selectedTpl) ? `<i class="fas fa-play"></i> Jalankan (${selected.size} × ${selectedTpl})` : '<i class="fas fa-play"></i> Jalankan Ansible';
}
async function loadTpls(){
  try {
    const tpls = await apiGet('/api/configure/templates');
    document.getElementById('tplRows').innerHTML = tpls.map(t => `
      <div class="col-md-4 mb-3"><div class="card h-100 ${selectedTpl === t.id ? 'border-left-primary shadow' : ''}" style="cursor:pointer" onclick="pickTpl('${t.id}')">
        <div class="card-body"><h6 class="font-weight-bold">${esc(t.name)}</h6>
        <div class="text-muted small">${esc(t.playbook)}</div><div class="small">${esc(t.note)}</div>
        ${selectedTpl === t.id ? '<span class="badge badge-primary mt-1"><i class="fas fa-check"></i> Dipilih</span>' : ''}</div></div></div>`).join('');
  } catch(e){ document.getElementById('tplRows').innerHTML = '<div class="text-danger">Gagal: ' + esc(e.message) + '</div>'; }
}
function pickTpl(id){ selectedTpl = (selectedTpl === id) ? null : id; loadTpls(); updateCount(); }
document.addEventListener('DOMContentLoaded', () => {
  loadSources().then(loadRows); loadTpls();
  document.getElementById('srcSel').onchange = loadRows;
  document.getElementById('btnAll').onclick = () => { rows.forEach(r => selected.add(r.key)); renderRows(); };
  document.getElementById('btnNone').onclick = () => { selected.clear(); renderRows(); };
  document.getElementById('btnRun').onclick = async () => {
    const box = document.getElementById('runBox'), log = document.getElementById('runLog'), title = document.getElementById('runTitle');
    box.style.display = ''; log.textContent = ''; title.textContent = 'Ansible berjalan... (realtime SSE)';
    try {
      const picked = rows.filter(r => selected.has(r.key));
      let body;
      if (srcMode === 'cluster') {
        const noIp = picked.filter(r => !r.ip);
        if (noIp.length) throw new Error('Host tanpa IP: ' + noIp.map(r => r.name).join(', ') + ' — pakai Static IP saat Deploy');
        body = {hosts: picked.map(r => ({name: r.name, ip: r.ip})), template: selectedTpl, ssh_user: val('sshUser')};
      } else {
        body = {vmids: picked.map(r => r.vmid), template: selectedTpl, ssh_user: val('sshUser')};
      }
      const r = await apiPost('/api/configure', body);
      await followRun(r.run_id, log, title);
      title.textContent = 'Selesai';
    } catch(e){ title.textContent = 'Gagal'; if (!log.textContent) log.textContent = e.message; }
  };
});
