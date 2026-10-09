/* shared helpers */
function goLogin(){ if(!location.pathname.includes('login')) location.href='/login.html'; }
async function apiGet(p){const r=await fetch(p);if(r.status===401){goLogin();throw new Error('login required');}if(!r.ok)throw new Error('GET '+p+' -> '+r.status);return r.json();}
async function apiPost(p,b){const r=await fetch(p,{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(b||{})});if(r.status===401){goLogin();throw new Error('login required');}const j=await r.json().catch(()=>({}));if(!r.ok)throw new Error(j.error||('POST '+p+' -> '+r.status));return j;}
function esc(s){return String(s??'').replace(/[&<>"']/g,c=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));}
function badgeFor(st){const m={pending:'secondary',deploying:'warning',provisioning:'warning',running:'success',error:'danger'};return m[st]||'secondary';}
function fmtMB(mb){if(mb==null)return '-';if(mb>=1024)return (mb/1024).toFixed(mb%1024?1:0)+' GB';return mb+' MB';}
function setConn(ok,text){const d=document.getElementById('connDot'),t=document.getElementById('connText');if(!d||!t)return;d.className='rounded-circle '+(ok==null?'bg-secondary':ok?'bg-success':'bg-danger');t.textContent=text||'';}
async function refreshConn(){
  try{
    const [h,s]=await Promise.all([apiGet('/api/health'),apiGet('/api/realtime/summary')]);
    const rt=s?.runtime||h?.runtime||{};
    const lbl=rt.label||(h?.wsl?.available?'Windows + WSL':'native');
    setConn(true,`${lbl} · ${s.clusters_total??0} clusters · ${s.running??0} running`);
  }catch(e){ setConn(false,'backend offline'); }
}
function markActive(){
  const p=location.pathname;
  // inject Settings nav on older pages if missing
  const sb=document.getElementById('accordionSidebar');
  if(sb && !document.getElementById('nav-settings')){
    const h=document.getElementById('nav-health');
    const li=document.createElement('li');
    li.className='nav-item'; li.id='nav-settings';
    li.innerHTML='<a class="nav-link" href="/settings.html"><i class="fas fa-cog"></i><span>Settings</span></a>';
    if(h && h.parentNode) h.parentNode.insertBefore(li, h.nextSibling);
    else sb.appendChild(li);
  }
  document.querySelectorAll('#accordionSidebar .nav-item').forEach(li=>li.classList.remove('active'));
  let id='nav-dash';
  if(p.includes('new-cluster'))id='nav-new';
  else if(p.includes('clone-vm'))id='nav-clone';
  else if(p.includes('configure'))id='nav-config';
  else if(p.includes('cluster-detail'))id='nav-dash';
  else if(p.includes('health'))id='nav-health';
  else if(p.includes('settings'))id='nav-settings';
  const el=document.getElementById(id);
  if(el)el.classList.add('active');
  injectLogout();
  guard();
}
async function guard(){
  if(location.pathname.includes('login'))return;
  try{
    const me=await apiGet('/api/me');
    const mu=document.getElementById('meUser');
    if(mu)mu.textContent=me.username||'';
    if(me.default_creds){
      const db=document.getElementById('defBadge');
      if(db)db.innerHTML='<span class="badge badge-warning">default password — change me</span>';
      const cf=document.querySelector('.container-fluid');
      if(cf && !document.getElementById('defWarn')){
        const d=document.createElement('div');
        d.id='defWarn';
        d.className='alert alert-warning';
        d.innerHTML='You are using the <strong>default password</strong>. Change it in <a href="/settings.html">Settings</a>.';
        cf.prepend(d);
      }
    }
  }catch(e){/* apiGet already redirects on 401 */}
}
function injectLogout(){
  if(location.pathname.includes('login'))return;
  const bar=document.querySelector('#content .topbar');
  if(!bar||document.getElementById('btnLogout'))return;
  const ul=document.createElement('ul');
  ul.className='navbar-nav ml-auto';
  ul.innerHTML='<li class="nav-item"><a id="btnLogout" class="nav-link" href="#" title="Logout"><i class="fas fa-sign-out-alt"></i> <span class="d-none d-lg-inline text-gray-600 small">Logout</span></a></li>';
  bar.appendChild(ul);
  document.getElementById('btnLogout').onclick=async(e)=>{
    e.preventDefault();
    await fetch('/api/logout',{method:'POST'});
    location.href='/login.html';
  };
}
document.addEventListener('DOMContentLoaded',()=>{markActive();refreshConn();setInterval(refreshConn,5000);});
function clusterRow(c){
  const id=encodeURIComponent(c.id);
  const masters=c.master_ips||[], workers=c.worker_ips||[];
  const ipTxt=(masters.length||workers.length)
    ? `<div class="small text-success">${esc(masters[0]||'-')} · +${workers.length}W</div>`
    : `<div class="small text-muted">IP belum tersimpan</div>`;
  return `<tr>
    <td><a href="/cluster-detail.html?id=${id}">${esc(c.name)}</a><div class="small text-muted">${esc(c.id).slice(0,8)}</div>${ipTxt}</td>
    <td>${esc(c.proxmox_url||'')}</td>
    <td>${c.master_count||1}M / ${c.worker_count||0}W</td>
    <td><span class="badge badge-${badgeFor(c.status)}">${esc(c.status||'pending')}</span></td>
    <td><a class="btn btn-sm btn-primary" href="/cluster-detail.html?id=${id}"><i class="fas fa-eye"></i> Open</a></td>
  </tr>`;
}
