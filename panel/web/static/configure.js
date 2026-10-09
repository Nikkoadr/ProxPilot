let allVMs = [], selected = new Set(), selectedTpl = null;
async function loadVMs(){
  try {
    const r = await apiGet('/api/vms');
    allVMs = (r.vms || []).filter(v => !v.template);
    const tb = document.getElementById('vmRows');
    if (!allVMs.length) { tb.innerHTML = '<tr><td colspan="6" class="text-muted">Tidak ada VM. Clone dulu di <a href="/clone">Clone VM</a>.</td></tr>'; return; }
    tb.innerHTML = allVMs.map(v => {
      const running = (v.status || '').toLowerCase() === 'running';
      return `<tr><td><input type="checkbox" class="form-check-input vm-chk" data-vmid="${v.vmid}" ${selected.has(v.vmid) ? 'checked' : ''}></td>
        <td><strong>${v.vmid}</strong></td><td>${esc(v.name)}</td>
        <td><span class="badge ${running ? 'bg-success' : 'bg-secondary'}">${esc(v.status)}</span></td>
        <td class="small">${((v.cpu||0)*100).toFixed(1)}%<br>${fmtMem(v.mem)} / ${fmtMem(v.maxmem)}</td>
        <td class="small">${fmtUptime(v.uptime)}</td></tr>`;
    }).join('');
    tb.querySelectorAll('.vm-chk').forEach(chk => chk.onchange = () => {
      const id = parseInt(chk.dataset.vmid, 10);
      chk.checked ? selected.add(id) : selected.delete(id);
      updateCount();
    });
    updateCount();
  } catch(e){
    document.getElementById('vmRows').innerHTML = '<tr><td colspan="6" class="text-danger">Gagal: ' + esc(e.message) + '</td></tr>';
  }
}
function updateCount(){
  document.getElementById('vmCount').textContent = selected.size + ' dipilih';
  const btn = document.getElementById('btnRun');
  btn.disabled = !(selected.size > 0 && selectedTpl);
  btn.textContent = (selected.size > 0 && selectedTpl) ? `Jalankan (${selected.size} VM × ${selectedTpl})` : 'Jalankan Ansible';
}
async function loadTpls(){
  try {
    const tpls = await apiGet('/api/configure/templates');
    document.getElementById('tplRows').innerHTML = tpls.map(t => `
      <div class="col-md-4 mb-2"><div class="card ${selectedTpl === t.id ? 'border-primary' : ''}" style="cursor:pointer" onclick="pickTpl('${t.id}')">
        <div class="card-body"><h4>${esc(t.name)}</h4>
        <div class="text-muted small">${esc(t.playbook)}</div><div class="small">${esc(t.note)}</div>
        ${selectedTpl === t.id ? '<span class="badge bg-primary mt-1">Dipilih</span>' : ''}</div></div></div>`).join('');
  } catch(e){ document.getElementById('tplRows').innerHTML = '<div class="text-danger">Gagal: ' + esc(e.message) + '</div>'; }
}
function pickTpl(id){ selectedTpl = (selectedTpl === id) ? null : id; loadTpls(); updateCount(); }
document.addEventListener('DOMContentLoaded', () => {
  loadVMs(); loadTpls();
  document.getElementById('btnAll').onclick = () => { allVMs.forEach(v => selected.add(v.vmid)); loadVMs(); };
  document.getElementById('btnNone').onclick = () => { selected.clear(); loadVMs(); };
  document.getElementById('btnRun').onclick = async () => {
    const box = document.getElementById('runBox'), log = document.getElementById('runLog'), title = document.getElementById('runTitle');
    box.style.display = ''; log.textContent = ''; title.textContent = 'Ansible berjalan...';
    try {
      const r = await apiPost('/api/configure', {vmids: [...selected], template: selectedTpl, ssh_user: val('sshUser')});
      await followRun(r.run_id, log, title);
      title.textContent = 'Selesai';
    } catch(e){ title.textContent = 'Gagal'; log.textContent += '\n' + e.message; }
  };
});
