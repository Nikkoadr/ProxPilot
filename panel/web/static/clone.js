async function loadTemplates(){
  try {
    const r = await apiGet('/api/templates');
    const list = r.templates || [];
    const sel = document.getElementById('cTemplate');
    sel.innerHTML = '<option value="">-- Pilih Template --</option>' + list.map(t =>
      `<option value="${esc(t.name)}">${esc(t.name)}${t.vmid ? ' (vmid ' + t.vmid + ')' : ''}${t.description ? ' — ' + esc(t.description) : ''}</option>`).join('');
    const badge = document.getElementById('tplBadge');
    const warn = document.getElementById('tplWarn');
    if (r.live && list.length) { badge.innerHTML = '<span class="badge badge-success">live</span>'; warn.textContent = list.length + ' template live. Node: ' + (r.node || '-'); }
    else if (r.source === 'live-empty') { badge.innerHTML = '<span class="badge badge-danger">kosong</span>'; warn.textContent = r.warning || 'Tidak ada template.'; }
    else { badge.innerHTML = '<span class="badge badge-warning">offline</span>'; warn.textContent = r.warning || ''; }
  } catch(e){
    document.getElementById('cTemplate').innerHTML = '<option value="">gagal load</option>';
    document.getElementById('tplWarn').textContent = e.message;
  }
}
async function loadVMs(){
  try {
    const r = await apiGet('/api/vms');
    const vms = (r.vms || []).filter(v => !v.template);
    document.getElementById('nodeLabel').textContent = r.node ? '@ ' + r.node : '';
    const tb = document.getElementById('vmRows');
    if (!vms.length) { tb.innerHTML = '<tr><td colspan="6" class="text-center text-muted">Belum ada VM. Clone dulu di atas.</td></tr>'; return; }
    tb.innerHTML = vms.map(v => {
      const running = (v.status || '').toLowerCase() === 'running';
      const btns = running
        ? `<button class="btn btn-sm btn-warning" onclick="vmAct(${v.vmid},'reboot',1)" title="Reboot"><i class="fas fa-redo"></i></button>
           <button class="btn btn-sm btn-secondary ml-1" onclick="vmAct(${v.vmid},'shutdown',1)" title="Shutdown"><i class="fas fa-power-off"></i></button>
           <button class="btn btn-sm btn-danger ml-1" onclick="vmAct(${v.vmid},'stop',1)" title="Stop"><i class="fas fa-stop"></i></button>
           <button class="btn btn-sm btn-outline-danger ml-1" onclick="vmDel(${v.vmid})" title="Hapus"><i class="fas fa-trash"></i></button>`
        : `<button class="btn btn-sm btn-success" onclick="vmAct(${v.vmid},'start',0)" title="Start"><i class="fas fa-play"></i></button>
           <button class="btn btn-sm btn-outline-danger ml-1" onclick="vmDel(${v.vmid})" title="Hapus"><i class="fas fa-trash"></i></button>`;
      return `<tr><td><strong>${v.vmid}</strong></td><td>${esc(v.name)}</td>
        <td><span class="badge ${running ? 'badge-success' : 'badge-secondary'}">${esc(v.status)}</span></td>
        <td class="small">${((v.cpu||0)*100).toFixed(1)}% / ${(v.cpus||'?')}c<br>${fmtMem(v.mem)} / ${fmtMem(v.maxmem)}</td>
        <td class="small">${fmtUptime(v.uptime)}</td><td>${btns}</td></tr>`;
    }).join('');
  } catch(e){
    document.getElementById('vmRows').innerHTML = '<tr><td colspan="6" class="text-center text-danger">Gagal: ' + esc(e.message) + '</td></tr>';
  }
}
async function vmAct(vmid, action, confirmIt){
  if (confirmIt && !(await confirmAct(action + ' VM ' + vmid + '?', ''))) return;
  try { await apiPost('/api/vms/' + vmid + '/' + action, {}); toast('success', action + ' ' + vmid); } catch(e){ Swal.fire('Gagal', e.message, 'error'); }
  loadVMs();
}
async function vmDel(vmid){
  if (!(await confirmAct('Hapus VM ' + vmid + '?', 'VM harus stopped dulu.'))) return;
  try { await apiDelete('/api/vms/' + vmid); toast('success', 'VM dihapus'); } catch(e){ Swal.fire('Gagal', e.message, 'error'); }
  loadVMs();
}
document.addEventListener('DOMContentLoaded', () => {
  loadTemplates(); loadVMs();
  document.getElementById('btnRefresh').onclick = () => { loadTemplates(); loadVMs(); };
  document.getElementById('btnClone').onclick = async () => {
    const btn = document.getElementById('btnClone'); btn.disabled = true;
    const box = document.getElementById('runBoxMain'), log = document.getElementById('runLog'), title = document.getElementById('runTitle');
    box.style.display = ''; log.textContent = ''; title.textContent = 'Cloning... (realtime SSE)';
    const vmidRaw = val('cVmid');
    try {
      const body = {template: val('cTemplate'), name: val('cName'),
        vmid: vmidRaw ? parseInt(vmidRaw, 10) : null, full: val('cFull') === '1',
        storage: val('cStorage') || 'local-lvm', static_ip: val('cStaticIp'), gateway: val('cGateway'),
        ciuser: val('cCiuser') || 'ubuntu', nameserver: val('cDns') || '8.8.8.8',
        start: document.getElementById('cStart').checked};
      if (!body.template) throw new Error('Pilih template dulu');
      if (!body.name) throw new Error('Nama VM wajib diisi');
      const r = await apiPost('/api/clone', body);
      $('#cloneModal').modal('hide');
      title.textContent = 'Clone berjalan...';
      await followRun(r.run_id, log, title);
      title.textContent = 'Selesai';
      document.getElementById('cName').value = '';
      loadTemplates(); loadVMs();
    } catch(e){ title.textContent = 'Gagal'; if (!log.textContent) log.textContent = e.message; }
    btn.disabled = false;
  };
});
