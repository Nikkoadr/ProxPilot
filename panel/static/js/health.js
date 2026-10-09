let step = { key: false, ssh: false, prox: false };

function setStep(n, ok) {
  step[n] = !!ok;
  const map = { key: 'stKey', ssh: 'stSsh', prox: 'stProx' };
  let el = document.getElementById(map[n]);
  if (el) el.className = 'badge ' + (ok ? 'badge-success' : 'badge-danger');
  el = document.getElementById('step' + n.charAt(0).toUpperCase() + n.slice(1));
  if (el) {
    const box = el.querySelector('.h4 span');
    if (box) box.className = 'badge ' + (ok ? 'badge-success' : 'badge-danger');
  }
  updateNextHint();
}

function updateNextHint() {
  const el = document.getElementById('nextHint');
  if (!el) return;
  if (!step.key) {
    el.innerHTML = '<span class="badge badge-warning">Langkah 1: Generate SSH key dulu</span>';
  } else if (!step.ssh) {
    el.innerHTML = '<span class="badge badge-info">Langkah 2: Test &amp; salin SSH key ke server Proxmox</span>';
  } else if (!step.prox) {
    el.innerHTML = '<span class="badge badge-info">Langkah 3: Test Proxmox API</span>';
  } else {
    el.innerHTML = '<a href="/clone-vm.html" class="btn btn-success btn-lg"><i class="fas fa-clone"></i> Lanjut ke Clone VM</a>';
  }
}

async function loadEnv() {
  try {
    const t = await apiGet('/api/tools');
    const rt = t.runtime || {};
    const wsl = t.wsl?.available;
    const native = rt.primary === 'local';
    let html = '';
    if (native) {
      html += `<div class="alert alert-success small"><i class="fas fa-check-circle"></i> <strong>Native Linux</strong> — tool lokal dipakai (terraform, ansible, ssh).</div>`;
    } else if (wsl) {
      const distro = (t.wsl.distros || '').split('\n').filter(l => l.trim()).slice(0, 2).join(' / ');
      html += `<div class="alert alert-info small"><i class="fas fa-info-circle"></i> <strong>Windows + WSL bridge</strong><br>Distro: ${esc(distro || 'default')}</div>`;
    } else {
      html += `<div class="alert alert-danger small"><i class="fas fa-exclamation-triangle"></i> <strong>WSL unavailable</strong> — install WSL atau jalankan panel di Linux.</div>`;
    }
    const row = (title, ok, sub) =>
      `<div class="d-inline-block mr-4"><span class="rounded-circle d-inline-block ${ok ? 'bg-success' : 'bg-danger'}" style="width:10px;height:10px"></span> <strong>${title}</strong>: ${esc(sub || '')}</div>`;
    const tf = native ? t.terraform?.local : t.terraform?.wsl;
    const an = native ? t.ansible?.local : t.ansible?.wsl;
    const sh = native ? t.ssh?.local : t.ssh?.wsl;
    html += `<div class="mt-2 small">${row('Terraform', !!tf?.ok, tf?.output)}${row('Ansible', !!an?.ok, an?.output)}${row('SSH', !!sh?.ok, sh?.output)}</div>`;
    document.getElementById('envBody').innerHTML = html;
  } catch (e) { console.warn(e); }
}

async function testProxmox() {
  const out = document.getElementById('proxOut');
  out.textContent = 'testing...';
  const body = {
    proxmox_url: document.getElementById('pUrl').value.trim(),
    proxmox_user: document.getElementById('pUser').value.trim() || 'root@pam',
    token_id: document.getElementById('pTokenId').value.trim(),
    token_secret: document.getElementById('pTokenSecret').value,
    verify_tls: document.getElementById('pVerify').checked,
  };
  try {
    const r = await apiPost('/api/health/proxmox-test', body);
    out.textContent = JSON.stringify(r, null, 2);
    out.style.borderLeft = r.ok ? '4px solid #1cc88a' : '4px solid #e74a3b';
    setStep('prox', !!r.ok);
  } catch (e) { out.textContent = String(e); setStep('prox', false); }
}

async function testSsh() {
  const out = document.getElementById('sshOut');
  out.textContent = 'testing ssh...';
  const body = {
    ssh_host: document.getElementById('sHost').value.trim(),
    ssh_user: document.getElementById('sUser').value.trim() || 'root',
    ssh_port: parseInt(document.getElementById('sPort').value, 10) || 22,
  };
  try {
    const r = await apiPost('/api/ssh/test', body);
    out.textContent = JSON.stringify(r, null, 2);
    out.style.borderLeft = r.ok ? '4px solid #1cc88a' : '4px solid #e74a3b';
    setStep('ssh', !!r.ok);
  } catch (e) { out.textContent = String(e); setStep('ssh', false); }
}

async function loadKey() {
  const st = document.getElementById('keyState');
  const pre = document.getElementById('pubKey');
  try {
    const k = await apiGet('/api/ssh/key');
    if (k.exists) {
      st.className = 'badge badge-success';
      st.textContent = 'ada (' + (k.via || '') + ')';
      pre.textContent = k.public_key || '';
      setStep('key', true);
    } else {
      st.className = 'badge badge-warning';
      st.textContent = 'belum ada';
      pre.textContent = '(belum ada — klik Generate key)';
      setStep('key', false);
    }
  } catch (e) {
    st.className = 'badge badge-danger';
    st.textContent = 'error';
    pre.textContent = String(e);
  }
}

async function genKey() {
  const out = document.getElementById('keyOut');
  out.textContent = 'generating...';
  try {
    await apiPost('/api/ssh/keygen', {});
    await loadKey();
    out.textContent = 'Key berhasil dibuat → lanjut langkah 2: salin key ke server Proxmox.';
    out.style.borderLeft = '4px solid #1cc88a';
  } catch (e) { out.textContent = String(e); out.style.borderLeft = '4px solid #e74a3b'; }
}

async function copyId() {
  const out = document.getElementById('sshOut');
  out.textContent = 'copying key to server...';
  const v = id => { const el = document.getElementById(id); return el ? el.value.trim() : ''; };
  try {
    const r = await apiPost('/api/ssh/copy-id', {
      ssh_host: v('sHost'),
      ssh_user: v('sUser') || 'root',
      ssh_port: parseInt(v('sPort'), 10) || 22,
      ssh_password: document.getElementById('cPass').value,
    });
    document.getElementById('cPass').value = '';
    out.textContent = (r.ok ? 'OK — key tersalin!\n' : 'GAGAL.\n') + JSON.stringify(r, null, 2);
    out.style.borderLeft = r.ok ? '4px solid #1cc88a' : '4px solid #e74a3b';
    if (r.ok) await testSsh();
  } catch (e) { out.textContent = String(e); out.style.borderLeft = '4px solid #e74a3b'; }
}

document.addEventListener('DOMContentLoaded', () => {
  loadEnv();
  setInterval(loadEnv, 10000);
  document.getElementById('btnProx').onclick = testProxmox;
  document.getElementById('btnSsh').onclick = testSsh;
  document.getElementById('btnKeygen').onclick = genKey;
  document.getElementById('btnCopyId').onclick = copyId;
  loadKey();
});
