// shared helpers
function goLogin(){ if (!location.pathname.includes('login')) location.href = '/login'; }
async function apiGet(p){
  const r = await fetch(p);
  if (r.status === 401) { goLogin(); throw new Error('login required'); }
  if (!r.ok) { let m = 'GET ' + p + ' -> ' + r.status; try { const j = await r.json(); if (j.error) m = j.error; } catch(e){} throw new Error(m); }
  return r.json();
}
async function apiPost(p, b){
  const r = await fetch(p, {method:'POST', headers:{'Content-Type':'application/json'}, body: JSON.stringify(b || {})});
  if (r.status === 401) { goLogin(); throw new Error('login required'); }
  const j = await r.json().catch(() => ({}));
  if (!r.ok) throw new Error(j.error || ('POST ' + p + ' -> ' + r.status));
  return j;
}
async function apiPut(p, b){
  const r = await fetch(p, {method:'PUT', headers:{'Content-Type':'application/json'}, body: JSON.stringify(b || {})});
  if (r.status === 401) { goLogin(); throw new Error('login required'); }
  const j = await r.json().catch(() => ({}));
  if (!r.ok) throw new Error(j.error || ('PUT ' + p + ' -> ' + r.status));
  return j;
}
async function apiDelete(p){
  const r = await fetch(p, {method:'DELETE'});
  if (r.status === 401) { goLogin(); throw new Error('login required'); }
  const j = await r.json().catch(() => ({}));
  if (!r.ok) throw new Error(j.error || ('DELETE ' + p + ' -> ' + r.status));
  return j;
}
function val(id){ const el = document.getElementById(id); return el ? el.value.trim() : ''; }
function esc(s){ return String(s ?? '').replace(/[&<>"']/g, c => ({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c])); }
function fmtMem(b){ if (b == null) return '-'; const m = Math.round(b / 1048576); return m >= 1024 ? (m/1024).toFixed(1) + ' GB' : m + ' MB'; }
function fmtUptime(s){ s = parseInt(s, 10) || 0; if (s < 60) return s + 's';
  const m = Math.floor(s/60), h = Math.floor(m/60), d = Math.floor(h/24);
  if (d) return d + 'd ' + (h%24) + 'h'; if (h) return h + 'h ' + (m%60) + 'm'; return m + 'm'; }
// SSE run follower -> appends lines into <pre>, resolves on done/error
function followRun(runId, logEl, titleEl){
  return new Promise((resolve, reject) => {
    const es = new EventSource('/api/runs/' + encodeURIComponent(runId) + '/events');
    const line = d => { logEl.textContent += d + '\n'; logEl.scrollTop = logEl.scrollHeight; };
    es.addEventListener('log', e => line(e.data));
    es.addEventListener('done', e => { if (e.data) line('== ' + e.data); es.close(); resolve(true); });
    es.addEventListener('error', e => { if (e.data) line('GAGAL: ' + e.data); es.close(); reject(new Error(e.data || 'run gagal')); });
    es.onerror = () => {};
  });
}
document.addEventListener('DOMContentLoaded', () => {
  const lo = document.getElementById('btnLogout');
  if (lo) lo.onclick = async e => { e.preventDefault(); await fetch('/api/logout', {method:'POST'}); location.href = '/login'; };
});
