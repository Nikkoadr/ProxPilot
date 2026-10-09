let id=new URLSearchParams(location.search).get('id');
let ws=null,pollTimer=null,lastCluster=null,connFilled=false;
function fillConn(c){
  if(connFilled||!c)return;
  const set=(eid,v)=>{const el=document.getElementById(eid);if(el)el.value=v??'';};
  set('eUrl',c.proxmox_url);set('eUser',c.proxmox_user);set('eTokenId',c.token_id);
  set('eNode',c.target_node);set('eTemplate',c.clone_template);
  set('eVmPrefix',c.vm_name_prefix);
  set('eDiskSize',c.disk_size_gb??0);set('eDiskStorage',c.disk_storage||'local-lvm');
  set('eVlan',c.vlan_tag??-1);
  set('eSshHost',c.ssh_host);set('eSshUser',c.ssh_remote_user||'root');set('eSshPort',c.ssh_port||22);
  const m=document.getElementById('eIpMode');if(m)m.value=c.ip_mode||'dhcp';
  set('eStaticBase',c.static_ip_base);
  connFilled=true;
}
async function saveConn(){
  const msg=document.getElementById('connMsg');msg.textContent='saving...';msg.className='small text-muted mb-1';
  const v=eid=>{const el=document.getElementById(eid);return el?el.value.trim():'';};
  const body=Object.assign({},lastCluster||{},{
    proxmox_url:v('eUrl'),proxmox_user:v('eUser')||'root@pam',token_id:v('eTokenId'),
    token_secret:document.getElementById('eTokenSecret').value,
    target_node:v('eNode'),clone_template:v('eTemplate'),
    vm_name_prefix:v('eVmPrefix'),
    disk_size_gb:Math.max(0,parseInt(v('eDiskSize'),10)||0),
    disk_storage:v('eDiskStorage'),
    vlan_tag:(()=>{const n=parseInt(v('eVlan'),10);return isNaN(n)?-1:Math.min(4094,Math.max(-1,n));})(),
    ssh_host:v('eSshHost'),ssh_remote_user:v('eSshUser')||'root',
    ssh_port:parseInt(v('eSshPort'),10)||22,
    ip_mode:(document.getElementById('eIpMode')||{}).value||'dhcp',
    static_ip_base:v('eStaticBase'),
  });
  try{
    const r=await fetch('/api/clusters/'+encodeURIComponent(id),{method:'PUT',headers:{'Content-Type':'application/json'},body:JSON.stringify(body)});
    if(r.status===401){goLogin();return;}
    const j=await r.json().catch(()=>({}));
    if(!r.ok)throw new Error(j.error||('PUT -> '+r.status));
    document.getElementById('eTokenSecret').value='';
    msg.textContent='Saved. Re-Deploy agar berlaku.';msg.className='small text-success mb-1';
    connFilled=false;poll();
  }catch(e){msg.textContent=String(e.message||e);msg.className='small text-danger mb-1';}
}
function logLine(l){
  const t=l.timestamp?new Date(l.timestamp).toLocaleTimeString():'';
  return `<div class="log-line"><span class="log-time">${esc(t)}</span><span class="log-level log-level-${esc(l.level||'info')}">[${esc(l.phase||'')}]</span> ${esc(l.line||'')}</div>`;
}
function render(data){
  const c=data.cluster||data;
  lastCluster=c;
  fillConn(c);
  renderIps(c);
  document.getElementById('cName').textContent=c.name||id;
  const feats=c.enabled_features||[];
  const fb=document.getElementById('cFeat');
  if(fb)fb.innerHTML=feats.length?feats.map(f=>`<span class="badge badge-info mr-1">${esc(f)}</span>`).join(''):'<span class="badge badge-secondary">common only</span>';
  document.getElementById('cMeta').textContent=`${c.master_count||1} master · ${c.worker_count||0} worker · ${esc(c.proxmox_url||'')}`;
  const b=document.getElementById('cStatus');
  b.className='badge badge-'+badgeFor(data.status);
  b.textContent=data.status;
  let sim=document.getElementById('cSim');
  if(data.simulated){
    if(!sim){sim=document.createElement('span');sim.id='cSim';b.after(sim);}
    sim.className='badge badge-warning ml-1';
    sim.title='Last deploy did not provision real VMs — see logs';
    sim.textContent='simulated';
  } else if(sim){sim.remove();}
  const bar=document.getElementById('cBar');
  bar.style.width=(data.progress||0)+'%';bar.textContent=(data.progress||0)+'%';
  document.getElementById('cProg').style.display=(data.status==='deploying'||data.status==='provisioning')?'':'none';
  const logs=data.logs||[];
  document.getElementById('logs').innerHTML=logs.length?logs.map(logLine).join(''):'<div class="text-muted">No logs yet. Click Deploy.</div>';
  const box=document.getElementById('logBox');box.scrollTop=box.scrollHeight;
  document.getElementById('btnDeploy').style.display=(data.status==='pending'||data.status==='error')?'':'none';
  document.getElementById('btnDestroy').style.display=(data.status==='running'||data.status==='error')?'':'none';
  const busy=(data.status==='deploying'||data.status==='provisioning');
  document.getElementById('btnPlan').style.display=busy?'none':'';
}
function renderIps(c){
  const masters=c.master_ips||[], workers=c.worker_ips||[];
  const rows=[];
  const nM=Math.max(c.master_count||1, masters.length);
  for(let i=0;i<nM;i++){
    const ip=masters[i]||'(belum ada — placeholder master-'+i+')';
    rows.push(`<tr><td><strong>master-${i}</strong></td><td>${esc(ip)}</td></tr>`);
  }
  const nW=Math.max(c.worker_count||0, workers.length);
  for(let i=0;i<nW;i++){
    const ip=workers[i]||('(belum ada — placeholder worker-'+i+')');
    rows.push(`<tr><td><strong>worker-${i}</strong></td><td>${esc(ip)}</td></tr>`);
  }
  const tb=document.getElementById('ipRows');
  if(tb)tb.innerHTML=rows.length?rows.join(''):'<tr><td colspan="2" class="text-center text-muted">-</td></tr>';
  const msg=document.getElementById('ipMsg');
  if(msg){
    if(masters.length||workers.length){
      msg.textContent=`Tersimpan di DB: ${masters.length} master, ${workers.length} worker. inventory.ini sudah pakai IP asli.`;
      msg.className='small mb-2 text-success';
    } else {
      msg.textContent='Belum ada IP — Deploy dulu, lalu IP DHCP otomatis tersimpan di sini. Kalau apply OK tapi masih kosong, klik Refresh IPs (tunggu qemu-guest-agent 1-3 mnt).';
      msg.className='small mb-2 text-muted';
    }
  }
  const sshUser=c.ssh_user||'ubuntu';
  const inv=[`[k8s_master]`];
  for(let i=0;i<(c.master_count||1);i++)inv.push(`master-${i} ansible_host=${esc(masters[i]||('master-'+i))} ansible_user=${esc(sshUser)}`);
  inv.push('',`[k8s_workers]`);
  for(let i=0;i<(c.worker_count||0);i++)inv.push(`worker-${i} ansible_host=${esc(workers[i]||('worker-'+i))} ansible_user=${esc(sshUser)}`);
  inv.push('',`[nginx_group]`,`master-0`,'',`[k8s_cluster:children]`,`k8s_master`,`k8s_workers`);
  const prev=document.getElementById('invPreview');
  if(prev)prev.textContent=inv.join('\n');
  const cmds=document.getElementById('ansibleCmds');
  if(cmds)cmds.textContent=`ansible-playbook -i inventory.ini ansible/playbook-master.yml && ansible-playbook -i inventory.ini ansible/playbook-workers.yml`;
}
async function refreshIps(){
  const msg=document.getElementById('ipMsg');
  if(msg){msg.textContent='refreshing terraform output...';msg.className='small mb-2 text-muted';}
  try{
    const r=await apiPost('/api/clusters/'+encodeURIComponent(id)+'/refresh-ips',{});
    if(msg){
      if(r.ok){msg.textContent=`OK: masters [${(r.master_ips||[]).join(', ')}] workers [${(r.worker_ips||[]).join(', ')}]`;msg.className='small mb-2 text-success';}
      else{msg.textContent=(r.hint||'terraform output masih kosong — tunggu agent/DHCP lalu coba lagi.');msg.className='small mb-2 text-warning';}
    }
    poll();
  }catch(e){if(msg){msg.textContent=String(e.message||e);msg.className='small mb-2 text-danger';}}
}
async function poll(){try{render(await apiGet('/api/clusters/'+encodeURIComponent(id)+'/status'));}catch(e){} loadVms();}
function fmtUptime(s){
  s=parseInt(s,10)||0;
  if(s<60)return s+'s';
  const m=Math.floor(s/60),h=Math.floor(m/60),d=Math.floor(h/24);
  if(d)return d+'d '+(h%24)+'h';
  if(h)return h+'h '+(m%60)+'m';
  return m+'m';
}
function fmtMem(mb){if(mb==null)return '-';const m=Math.round(mb/1048576);return m>=1024?(m/1024).toFixed(1)+' GB':m+' MB';}
async function loadVms(){
  const tb=document.getElementById('vmRows');
  try{
    const r=await apiGet('/api/clusters/'+encodeURIComponent(id)+'/vms');
    const vms=(r&&r.vms)||[];
    if(!vms.length){tb.innerHTML='<tr><td colspan="6" class="text-center text-muted">Belum ada VM (Deploy dulu).</td></tr>';return;}
    tb.innerHTML=vms.map(v=>{
      const running=(v.status||'').toLowerCase()==='running';
      const badge=running?'success':'secondary';
      const cpu=((v.cpu||0)*100).toFixed(1)+'% / '+(v.cpus||'?')+'c';
      const mem=fmtMem(v.mem)+' / '+fmtMem(v.maxmem);
      let btns='';
      if(running){
        btns=`<button class="btn btn-sm btn-warning" onclick="vmAct(${v.vmid},'reboot',1)" title="Reboot"><i class="fas fa-redo"></i></button>
        <button class="btn btn-sm btn-secondary ml-1" onclick="vmAct(${v.vmid},'shutdown',1)" title="Shutdown (graceful)"><i class="fas fa-power-off"></i></button>
        <button class="btn btn-sm btn-danger ml-1" onclick="vmAct(${v.vmid},'stop',1)" title="Stop (paksa)"><i class="fas fa-stop"></i></button>`;
      }else{
        btns=`<button class="btn btn-sm btn-success" onclick="vmAct(${v.vmid},'start',0)" title="Start"><i class="fas fa-play"></i></button>`;
      }
      return `<tr><td><strong>${esc(v.name)}</strong></td><td>${v.vmid}</td>
        <td><span class="badge badge-${badge}">${esc(v.status)}</span></td>
        <td class="small">${cpu}<br>${mem}</td><td class="small">${fmtUptime(v.uptime)}</td><td>${btns}</td></tr>`;
    }).join('');
  }catch(e){tb.innerHTML='<tr><td colspan="6" class="text-center text-muted">VM list unavailable (cek koneksi Proxmox).</td></tr>';}
}
async function preflight(){
  const card=document.getElementById('preCard'), rows=document.getElementById('preRows');
  card.style.display='';
  rows.innerHTML='<div class="text-muted">mengecek api → template → terraform/ssh → ip...</div>';
  try{
    const r=await apiPost('/api/clusters/'+encodeURIComponent(id)+'/preflight',{});
    rows.innerHTML=(r.checks||[]).map(c=>
      `<div><span class="badge badge-${c.ok?'success':'danger'}">${c.ok?'OK':'FAIL'}</span> <strong>${esc(c.name)}</strong> <span class="small text-muted">${esc(c.detail||'')}</span></div>`
    ).join('')+`<div class="mt-1 small ${r.ok?'text-success':'text-danger'}">${r.ok?'Siap Deploy.':'Perbaiki yang FAIL dulu, baru Deploy.'}</div>`;
  }catch(e){rows.innerHTML='<div class="text-danger">'+esc(String(e.message||e))+'</div>';}
}
async function vmAct(vmid,action,confirmIt){
  if(confirmIt&&!confirm(action+' VM '+vmid+'?'))return;
  try{await apiPost('/api/clusters/'+encodeURIComponent(id)+'/vms/'+vmid+'/'+action,{});}
  catch(e){alert(String(e.message||e));}
  loadVms();poll();
}
function connectWs(){
  try{ws&&ws.close();}catch(e){}
  const proto=location.protocol==='https:'?'wss':'ws';
  ws=new WebSocket(`${proto}://${location.host}/api/ws/logs/${encodeURIComponent(id)}`);
  ws.onopen=()=>ws.send(JSON.stringify({action:'subscribe'}));
  ws.onmessage=(ev)=>{
    try{
      const m=JSON.parse(ev.data);
      if(m.type==='logs'){poll();}
      else if(m.type==='log'&&m.entry){
        const box=document.getElementById('logs');
        box.insertAdjacentHTML('beforeend',logLine(m.entry));
        document.getElementById('logBox').scrollTop=1e9;
        if(m.status){const b=document.getElementById('cStatus');b.className='badge badge-'+badgeFor(m.status);b.textContent=m.status;
          const bar=document.getElementById('cBar');bar.style.width=(m.progress||0)+'%';bar.textContent=(m.progress||0)+'%';}
      }
      else if(m.type==='heartbeat'&&m.status){
        const b=document.getElementById('cStatus');b.className='badge badge-'+badgeFor(m.status);b.textContent=m.status;
      }
    }catch(e){}
  };
  ws.onclose=()=>setTimeout(connectWs,3000);
}
document.addEventListener('DOMContentLoaded',()=>{
  if(!id){document.getElementById('cName').textContent='missing ?id=';return;}
  document.getElementById('btnDeploy').onclick=async()=>{
    try{await apiPost('/api/clusters/'+encodeURIComponent(id)+'/deploy',{});poll();}
    catch(e){alert(String(e.message||e));}
  };
  document.getElementById('btnPlan').onclick=async()=>{
    try{await apiPost('/api/clusters/'+encodeURIComponent(id)+'/plan',{});poll();}
    catch(e){alert(String(e.message||e));}
  };
  document.getElementById('btnPreflight').onclick=preflight;
  document.getElementById('btnDestroy').onclick=async()=>{
    if(!confirm('Destroy SEMUA VM cluster ini di Proxmox? Definisi cluster tetap tersimpan (bisa Deploy ulang).'))return;
    try{await apiPost('/api/clusters/'+encodeURIComponent(id)+'/destroy',{});poll();}
    catch(e){alert(String(e.message||e));}
  };
  document.getElementById('btnDelete').onclick=async()=>{if(!confirm('Delete this cluster record? (Hancurkan VM dulu via Destroy VMs bila masih ada — Delete hanya hapus data panel)'))return;await fetch('/api/clusters/'+encodeURIComponent(id),{method:'DELETE'});location.href='/index.html';};
  document.getElementById('btnSaveConn').onclick=saveConn;
  document.getElementById('btnRefreshIps').onclick=refreshIps;
  document.getElementById('btnVmRefresh').onclick=loadVms;
  document.getElementById('btnConfigure').href='/configure.html?id='+encodeURIComponent(id);
  poll();pollTimer=setInterval(poll,3000);connectWs();
});
