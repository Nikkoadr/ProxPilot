async function loadSetup(){
  try {
    const s = await apiGet('/api/setup');
    if (s.proxmox_url) document.getElementById('pUrl').value = s.proxmox_url;
    if (s.proxmox_user) document.getElementById('pUser').value = s.proxmox_user;
    if (s.token_id) document.getElementById('pTokenId').value = s.token_id;
    if (s.target_node) document.getElementById('pNode').value = s.target_node;
    if (s.verify_tls) document.getElementById('pVerify').checked = true;
    if (s.ssh_host) document.getElementById('sHost').value = s.ssh_host;
    if (s.ssh_user) document.getElementById('sUser').value = s.ssh_user;
    if (s.ssh_port) document.getElementById('sPort').value = s.ssh_port;
    document.getElementById('setupOut').textContent = s.saved ? 'tersimpan' + (s.complete ? ' (lengkap — Clone & Configure terbuka)' : ' (belum lengkap)') : 'belum disimpan';
    gate(s);
  } catch(e){}
  loadKey();
}
function gate(s){
  const el = document.getElementById('gateAlert');
  if (s && s.complete) {
    el.innerHTML = '<div class="alert alert-success">Koneksi lengkap. Menu <a href="/clone">Clone VM</a> &amp; <a href="/configure">Configure</a> terbuka.</div>';
  } else {
    el.innerHTML = '<div class="alert alert-warning">Selesaikan Test SSH + Test API, lalu Simpan agar menu lain terbuka.</div>';
  }
}
async function loadKey(){
  try {
    const k = await apiGet('/api/ssh/key');
    const st = document.getElementById('keyState');
    if (k.exists) { st.className = 'badge bg-success'; st.textContent = 'ada'; document.getElementById('pubKey').textContent = k.public_key || ''; }
    else { st.className = 'badge bg-warning'; st.textContent = 'belum ada'; }
  } catch(e){}
}
async function testSSH(){
  const out = document.getElementById('sshOut'); out.textContent = 'testing...';
  try {
    const r = await apiPost('/api/test/ssh', {ssh_host: val('sHost'), ssh_user: val('sUser') || 'root', ssh_port: parseInt(val('sPort')) || 22});
    out.textContent = (r.ok ? 'OK\n' : 'GAGAL\n') + (r.output || '');
  } catch(e){ out.textContent = String(e.message || e); }
}
async function testProxmox(){
  const out = document.getElementById('proxOut'); out.textContent = 'testing...';
  try {
    const r = await apiPost('/api/test/proxmox', {proxmox_url: val('pUrl'), proxmox_user: val('pUser') || 'root@pam',
      token_id: val('pTokenId'), token_secret: document.getElementById('pTokenSecret').value,
      verify_tls: document.getElementById('pVerify').checked});
    out.textContent = JSON.stringify(r, null, 2);
    if (r.ok) detectNodes(true);
  } catch(e){ out.textContent = String(e.message || e); }
}
async function detectNodes(silent){
  const hint = document.getElementById('nodeHint');
  try {
    const nodes = await apiGet('/api/nodes');
    const live = (nodes || []).filter(n => n.live !== false && !String(n.status || '').includes('mock'));
    if (!live.length) { if (hint) hint.textContent = 'Belum ada node live — simpan koneksi yang valid dulu.'; return; }
    if (hint) hint.textContent = 'Node live: ' + live.map(n => n.name).join(', ');
    const inp = document.getElementById('pNode');
    if (inp && !inp.value.trim()) inp.value = live[0].name;
  } catch(e){ if (!silent && hint) hint.textContent = 'Gagal: ' + e.message; }
}
async function saveSetup(){
  const out = document.getElementById('setupOut'); out.textContent = 'menyimpan...';
  try {
    const r = await apiPut('/api/setup', {proxmox_url: val('pUrl'), proxmox_user: val('pUser') || 'root@pam',
      token_id: val('pTokenId'), token_secret: document.getElementById('pTokenSecret').value,
      verify_tls: document.getElementById('pVerify').checked, target_node: val('pNode') || 'pve',
      ssh_host: val('sHost'), ssh_user: val('sUser') || 'root', ssh_port: parseInt(val('sPort')) || 22});
    document.getElementById('pTokenSecret').value = '';
    out.textContent = 'OK — ' + (r.message || 'tersimpan');
    loadSetup();
  } catch(e){ out.textContent = String(e.message || e); }
}
document.addEventListener('DOMContentLoaded', () => {
  loadSetup();
  document.getElementById('btnKeygen').onclick = async () => { try { await apiPost('/api/ssh/keygen', {}); loadKey(); } catch(e){ alert(e.message); } };
  document.getElementById('btnCopyId').onclick = async () => {
    const out = document.getElementById('sshOut'); out.textContent = 'copying...';
    try {
      const r = await apiPost('/api/ssh/copy', {ssh_host: val('sHost'), ssh_user: val('sUser') || 'root',
        ssh_port: parseInt(val('sPort')) || 22, ssh_password: document.getElementById('cPass').value});
      document.getElementById('cPass').value = '';
      out.textContent = (r.ok ? 'OK\n' : 'GAGAL\n') + (r.output || '');
      if (r.ok) testSSH();
    } catch(e){ out.textContent = String(e.message || e); }
  };
  document.getElementById('btnSsh').onclick = testSSH;
  document.getElementById('btnProx').onclick = testProxmox;
  document.getElementById('btnNodes').onclick = () => detectNodes(false);
  document.getElementById('btnSave').onclick = saveSetup;
});
