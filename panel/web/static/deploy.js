let liveTemplates = [];
let liveNodes = [];
async function loadNodes(){
  try {
    const nodes = await apiGet('/api/nodes');
    liveNodes = (nodes || []).filter(n => n.name && n.name !== '?');
    const sel = document.getElementById('fNode');
    if (liveNodes.length) {
      sel.innerHTML = liveNodes.map(n => `<option value="${esc(n.name)}">${esc(n.name)}</option>`).join('');
    } else {
      sel.innerHTML = '<option value="pve">pve</option>';
    }
  } catch(e){ document.getElementById('fNode').innerHTML = '<option value="pve">pve</option>'; }
}
async function loadTemplates(){
  try {
    const r = await apiGet('/api/templates');
    liveTemplates = r.templates || [];
    const sel = document.getElementById('fTemplate');
    sel.innerHTML = '<option value="">-- Pilih Template --</option>' + liveTemplates.map(t =>
      `<option value="${t.vmid}" data-name="${esc(t.name)}">${esc(t.name)} (vmid ${t.vmid})</option>`).join('');
    document.getElementById('tplBadge').innerHTML = r.live && liveTemplates.length
      ? '<span class="badge badge-success">live</span>' : '<span class="badge badge-danger">kosong</span>';
  } catch(e){ document.getElementById('fTemplate').innerHTML = '<option value="">gagal load</option>'; }
}
async function loadClusters(){
  try {
    const list = await apiGet('/api/clusters');
    const tb = document.getElementById('clusterRows');
    if (!list.length) { tb.innerHTML = '<tr><td colspan="6" class="text-center text-muted">Belum ada cluster.</td></tr>'; return; }
    tb.innerHTML = list.map(c => {
      const badge = {pending:'secondary', deploying:'warning', running:'success', error:'danger'}[c.status] || 'secondary';
      const n = (c.vm_specs && c.vm_specs.length) || (c.vm_names && c.vm_names.length) || 0;
      return `<tr><td><strong>${esc(c.name)}</strong><div class="small text-muted">${esc(c.node)}</div></td>
        <td class="small">${esc(c.template_name || c.template_vmid)}</td>
        <td class="small">${n} VM</td>
        <td class="small">${c.ip_mode}${c.ip_mode === 'static' ? '<br>' + esc(c.base_ip) : ''}</td>
        <td><span class="badge badge-${badge}">${esc(c.status)}</span></td>
        <td><button class="btn btn-sm btn-success" onclick="deployCluster('${c.id}')"><i class="fas fa-rocket"></i> Deploy</button>
        <button class="btn btn-sm btn-info ml-1" onclick="showHosts('${c.id}','${esc(c.name)}')">Hosts</button>
        <button class="btn btn-sm btn-warning ml-1" onclick="destroyCluster('${c.id}')">Destroy</button>
        <button class="btn btn-sm btn-outline-danger ml-1" onclick="delCluster('${c.id}')"><i class="fas fa-trash"></i></button></td></tr>`;
    }).join('');
  } catch(e){
    document.getElementById('clusterRows').innerHTML = '<tr><td colspan="6" class="text-center text-danger">' + esc(e.message) + '</td></tr>';
  }
}
async function showHosts(id, name){
  try {
    const hosts = await apiGet('/api/clusters/' + encodeURIComponent(id) + '/hosts');
    const box = document.getElementById('hostBox');
    if (!hosts.length) { box.innerHTML = '<div class="alert alert-warning mb-0">Cluster ' + esc(name) + ' belum punya hosts — Deploy dulu.</div>'; return; }
    box.innerHTML = '<h6>Hosts ' + esc(name) + ' (bisa dipakai di <a href="/configure">Configure</a>)</h6>' +
      '<table class="table table-sm table-bordered"><thead><tr><th>VM</th><th>VMID</th><th>IP</th></tr></thead><tbody>' +
      hosts.map(h => `<tr><td>${esc(h.vm_name)}</td><td>${h.vmid}</td><td><strong>${esc(h.ip || '-')}</strong></td></tr>`).join('') + '</tbody></table>';
  } catch(e){ toast('error', e.message); }
}
async function deployCluster(id){
  const box = document.getElementById('runBox'), log = document.getElementById('runLog'), title = document.getElementById('runTitle');
  box.style.display = ''; log.textContent = ''; title.textContent = 'Deploy berjalan... (realtime SSE)';
  try {
    const r = await apiPost('/api/clusters/' + encodeURIComponent(id) + '/deploy', {});
    await followRun(r.run_id, log, title);
    title.textContent = 'Selesai'; loadClusters();
  } catch(e){ title.textContent = 'Gagal'; }
}
async function destroyCluster(id){
  if (!(await confirmAct('Destroy semua VM cluster ini?', 'Definisi cluster tetap ada.'))) return;
  try {
    const r = await apiPost('/api/clusters/' + encodeURIComponent(id) + '/destroy', {});
    toast('info', 'Destroy berjalan, lihat log di Deploy...');
    loadClusters();
  } catch(e){ Swal.fire('Gagal', e.message, 'error'); }
}
async function delCluster(id){
  if (!(await confirmAct('Hapus definisi cluster?', 'File terraform ikut dihapus.'))) return;
  try { await apiDelete('/api/clusters/' + encodeURIComponent(id)); loadClusters(); }
  catch(e){ Swal.fire('Gagal', e.message, 'error'); }
}
function renderNameInputs(){
  const n = Math.min(20, Math.max(1, parseInt(val('fCount')) || 1));
  const prefix = val('fName') || 'vm';
  const box = document.getElementById('vmForms');
  let html = '';
  for (let i = 0; i < n; i++) {
    html += `<div class="card mb-2 vm-form"><div class="card-body py-2"><div class="form-row align-items-end">
      <div class="form-group col-md-3 mb-1"><label class="small">Nama VM ${i + 1}</label>
        <input class="form-control form-control-sm vm-f-name" value="${esc(prefix)}-${i + 1}" maxlength="63"></div>
      <div class="form-group col-md-1 mb-1"><label class="small">CPU</label><input type="number" class="form-control form-control-sm vm-f-cpu" value="2" min="1"></div>
      <div class="form-group col-md-2 mb-1"><label class="small">RAM MB</label><input type="number" class="form-control form-control-sm vm-f-ram" value="4096" step="512"></div>
      <div class="form-group col-md-1 mb-1"><label class="small">Disk</label><input type="number" class="form-control form-control-sm vm-f-disk" value="32" min="4"></div>
      <div class="form-group col-md-2 mb-1"><label class="small">Bridge</label><input class="form-control form-control-sm vm-f-bridge" value="vmbr0"></div>
      <div class="form-group col-md-1 mb-1"><label class="small">IP</label><select class="form-control form-control-sm vm-f-ipmode" onchange="toggleIp(this)"><option value="dhcp">DHCP</option><option value="static">Static</option></select></div>
      <div class="form-group col-md-2 mb-1 vm-f-ipwrap" style="display:none"><label class="small">Static IP</label><input class="form-control form-control-sm vm-f-ip" placeholder="192.168.1.50"></div>
    </div></div></div>`;
  }
  box.innerHTML = html;
}
function toggleIp(sel){
  const wrap = sel.closest('.vm-form').querySelector('.vm-f-ipwrap');
  wrap.style.display = sel.value === 'static' ? '' : 'none';
}
document.addEventListener('DOMContentLoaded', () => {
  loadNodes(); loadTemplates(); loadClusters();
  document.getElementById('fCount').onchange = renderNameInputs;
  document.getElementById('fName').oninput = renderNameInputs;
  renderNameInputs();
  document.getElementById('btnCreate').onclick = async () => {
    try {
      const sel = document.getElementById('fTemplate');
      const opt = sel.options[sel.selectedIndex] || {};
      const vms = [...document.querySelectorAll('.vm-form')].map(card => ({
        name: card.querySelector('.vm-f-name').value.trim(),
        cpu: parseInt(card.querySelector('.vm-f-cpu').value) || 2,
        ram: parseInt(card.querySelector('.vm-f-ram').value) || 4096,
        disk_gb: parseInt(card.querySelector('.vm-f-disk').value) || 32,
        bridge: card.querySelector('.vm-f-bridge').value.trim() || 'vmbr0',
        ip_mode: card.querySelector('.vm-f-ipmode').value,
        ip: card.querySelector('.vm-f-ip').value.trim(),
      }));
      const body = {name: val('fName'), node: val('fNode'), template_vmid: parseInt(sel.value) || 0,
        template_name: opt.dataset ? (opt.dataset.name || '') : '',
        vms: vms, ip_mode: 'dhcp', base_ip: ''};
      if (!body.name) throw new Error('Nama cluster wajib diisi');
      if (!body.template_vmid) throw new Error('Pilih template dulu');
      const c = await apiPost('/api/clusters', body);
      toast('success', 'Cluster dibuat');
      document.getElementById('fName').value = '';
      $('#clusterModal').modal('hide');
      loadClusters();
      deployCluster(c.id);
    } catch(e){ Swal.fire('Gagal', e.message, 'error'); }
  };
});
