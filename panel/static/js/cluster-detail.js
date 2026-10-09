let id=new URLSearchParams(location.search).get('id');
let ws=null,pollTimer=null,lastCluster=null,connFilled=false;
function fillConn(c){
  if(connFilled||!c)return;
  const set=(eid,v)=>{const el=document.getElementById(eid);if(el)el.value=v??'';};
  set('eUrl',c.proxmox_url);set('eUser',c.proxmox_user);set('eTokenId',c.token_id);
  set('eNode',c.target_node);set('eTemplate',c.clone_template);
  set('eVmPrefix',c.vm_name_prefix);
  set('eSshHost',c.ssh_host);set('eSshUser',c.ssh_remote_user||'root');set('eSshPort',c.ssh_port||22);
  const m=document.getElementById('eIpMode');if(m)m.value=c.ip_mode||'dhcp';
  set('eStaticBase',c.static_ip_base);
  const feats=c.enabled_features||[];
  const chk=(eid,f)=>{const el=document.getElementById(eid);if(el)el.checked=feats.includes(f);};
  chk('eFeatK8s','k8s');chk('eFeatNginx','nginx');chk('eFeatNode','nodejs');
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
    ssh_host:v('eSshHost'),ssh_remote_user:v('eSshUser')||'root',
    ssh_port:parseInt(v('eSshPort'),10)||22,
    ip_mode:(document.getElementById('eIpMode')||{}).value||'dhcp',
    static_ip_base:v('eStaticBase'),
    enabled_features:collectFeat(),
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
}
function collectFeat(){
  const out=[];
  if(document.getElementById('eFeatK8s')?.checked)out.push('k8s');
  if(document.getElementById('eFeatNginx')?.checked)out.push('nginx');
  if(document.getElementById('eFeatNode')?.checked)out.push('nodejs');
  return out;
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
async function poll(){try{render(await apiGet('/api/clusters/'+encodeURIComponent(id)+'/status'));}catch(e){}}
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
  document.getElementById('btnDeploy').onclick=async()=>{await apiPost('/api/clusters/'+encodeURIComponent(id)+'/deploy',{});poll();};
  document.getElementById('btnDelete').onclick=async()=>{if(!confirm('Delete this cluster?'))return;await fetch('/api/clusters/'+encodeURIComponent(id),{method:'DELETE'});location.href='/index.html';};
  document.getElementById('btnSaveConn').onclick=saveConn;
  document.getElementById('btnRefreshIps').onclick=refreshIps;
  poll();pollTimer=setInterval(poll,3000);connectWs();
});
