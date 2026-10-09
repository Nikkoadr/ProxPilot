async function init(){
  try{const c=await apiGet('/api/config');fill(c.defaults||{});}catch(e){}
  try{const t=await apiGet('/api/templates');fillTemplates(t);}catch(e){}
  try{
    const s=await apiGet('/api/setup');
    if(s && (s.proxmox_url || s.target_node)){
      const set=(id,v)=>{const el=document.getElementById(id);if(el&&el.value===''&&v)el.value=v;};
      set('proxmox_url',s.proxmox_url);set('proxmox_user',s.proxmox_user);
      set('token_id',s.token_id);set('target_node',s.target_node);
      set('ssh_host',s.ssh_host);set('ssh_user_remote',s.ssh_user);
      if(s.ssh_port){const el=document.getElementById('ssh_port');if(el&&el.value==='22')el.value=s.ssh_port;}
      if(s.ssh_host){const el=document.getElementById('useRemote');if(el)el.checked=true;toggleRemote();}
    }
  }catch(e){}
  document.getElementById('btnProxmox').onclick=testProxmox;
  document.getElementById('btnSsh').onclick=testSsh;
  document.getElementById('form').onsubmit=submit;
  document.getElementById('useRemote').onchange=toggleRemote;
  document.getElementById('clone_template').onchange=onTemplate;
  toggleRemote();
}
function templateVal(){
  const s=document.getElementById('clone_template');
  if(s&&s.value==='__custom'){const c=document.getElementById('clone_template_custom');return c?c.value.trim():'';}
  return val('clone_template');
}
function onTemplate(){
  const s=document.getElementById('clone_template'), c=document.getElementById('clone_template_custom');
  if(c)c.style.display=(s&&s.value==='__custom')?'':'none';
  // autosuggest SSH user VM (jangan timpa kalau user sudah edit manual)
  const u=document.getElementById('ssh_user_vm');
  if(!u||u.dataset.touched)return;
  const t=(s?s.value:'').toLowerCase();
  if(t.includes('rocky'))u.value='rocky';
  else if(t.includes('debian'))u.value='admin';
  else if(t.includes('ubuntu'))u.value='ubuntu';
}
function val(id){const el=document.getElementById(id);return el?el.value.trim():'';}
function num(id,d){const el=document.getElementById(id);const v=el?parseInt(el.value,10):NaN;return isNaN(v)?d:v;}
function fill(d){
  const set=(id,v)=>{const el=document.getElementById(id);if(el&&el.value===''&&v!=null)el.value=v;};
  set('proxmox_url',d.proxmox_url);set('proxmox_user',d.proxmox_user);
  set('master_cpu',d.master_cpu);set('master_ram',d.master_ram);
  set('worker_cpu',d.worker_cpu);set('worker_ram',d.worker_ram);
}
function fillTemplates(t){
  const s=document.getElementById('clone_template');
  // Backend baru: {templates, live, source}. Backend lama: array langsung.
  const list = Array.isArray(t) ? t : (t && Array.isArray(t.templates) ? t.templates : []);
  const live = t && !Array.isArray(t) ? !!t.live : list.some(x => x.vmid != null);
  s.innerHTML='<option value="">Select template...</option>'+(list||[]).map(x=>`<option value="${esc(x.name)}">${esc(x.name)} (${esc(x.description||x.size||'')})${x.vmid?' [vmid '+x.vmid+']':''}</option>`).join('')
    +'<option value="__custom">⌨ Custom / ketik manual...</option>';
  let hint=document.getElementById('tplHint');
  if(!live){
    if(!hint){hint=document.createElement('small');hint.id='tplHint';hint.className='form-text text-warning';s.parentNode.appendChild(hint);}
    hint.textContent='Fallback statis ('+list.length+' template) — bukan live Proxmox. Test API di bawah agar daftar live muncul.';
  } else if(hint){hint.textContent='';hint.className='form-text text-success';hint.textContent=list.length+' template live dari Proxmox.';}
  const u=document.getElementById('ssh_user_vm');
  if(u)u.oninput=()=>{u.dataset.touched='1';};
}
function toggleRemote(){
  const on=document.getElementById('useRemote').checked;
  ['ssh_host','ssh_user_remote','ssh_port'].forEach(id=>{const el=document.getElementById(id);if(el)el.disabled=!on;});
}
function proxmoxPayload(){
  return {proxmox_url:val('proxmox_url'),proxmox_user:val('proxmox_user')||'root@pam',
    token_id:val('token_id'),token_secret:document.getElementById('token_secret').value,
    verify_tls:document.getElementById('verify_tls').checked};
}
async function testProxmox(){
  const out=document.getElementById('proxOut');
  out.textContent='testing...';
  try{const r=await apiPost('/api/health/proxmox-test',proxmoxPayload());
    out.textContent=JSON.stringify(r,null,2);
    out.className='out '+(r.ok?'border-left-success':'border-left-danger');
  }catch(e){out.textContent=String(e);}
}
async function testSsh(){
  const out=document.getElementById('sshOut');
  out.textContent='testing ssh...';
  try{const r=await apiPost('/api/ssh/test',{ssh_host:val('ssh_host'),ssh_user:val('ssh_user_remote')||'root',ssh_port:num('ssh_port',22)});
    out.textContent=JSON.stringify(r,null,2);
  }catch(e){out.textContent=String(e);}
}
async function submit(e){
  e.preventDefault();
  const err=document.getElementById('err');
  err.textContent='';
  const useRemote=document.getElementById('useRemote').checked;
  const body={
    id:'',name:val('name'),vm_name_prefix:val('vm_name_prefix'),
    disk_size_gb:num('disk_size_gb',0),disk_storage:val('disk_storage')||'local-lvm',
    vlan_tag:(()=>{const v=num('vlan_tag',-1);return (v>=-1&&v<=4094)?v:-1;})(),
    proxmox_url:val('proxmox_url'),proxmox_user:val('proxmox_user')||'root@pam',
    token_id:val('token_id'),token_secret:document.getElementById('token_secret').value,
    verify_tls:document.getElementById('verify_tls').checked,
    target_node:val('target_node')||'pve',clone_template:templateVal(),
    network_bridge:val('network_bridge')||'vmbr0',gateway:val('gateway')||'192.168.1.1',dns1:val('dns1')||'8.8.8.8',
    master_count:num('master_count',1),master_cpu:num('master_cpu',4),master_ram:num('master_ram',8192),
    worker_count:num('worker_count',2),worker_cpu:num('worker_cpu',2),worker_ram:num('worker_ram',4096),
    ssh_user:val('ssh_user_vm')||'ubuntu',ssh_public_key:'',
    ip_mode:val('ip_mode')||'dhcp',static_ip_base:val('static_ip_base'),
    ssh_host:useRemote?val('ssh_host'):'',ssh_port:num('ssh_port',22),
    ssh_remote_user:useRemote?(val('ssh_user_remote')||'root'):'',
    use_wsl:true,status:'pending',progress:0,
    enabled_features:[],created_at:new Date().toISOString(),updated_at:new Date().toISOString()
  };
  if(!body.name){err.textContent='Cluster name required';return;}
  if(!body.token_secret){err.textContent='API Token secret required';return;}
  if(!body.clone_template){err.textContent='Select a template';return;}
  if(body.ip_mode==='static'&&!/^\d{1,3}(\.\d{1,3}){3}$/.test(body.static_ip_base)){err.textContent='Static base IP required (format A.B.C.D, mis. 192.168.1.50)';return;}
  try{
    const c=await apiPost('/api/clusters',body);
    location.href='/cluster-detail.html?id='+encodeURIComponent(c.id);
  }catch(e){err.textContent=String(e);}
}
document.addEventListener('DOMContentLoaded',init);
