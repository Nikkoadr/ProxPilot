let id=new URLSearchParams(location.search).get('id');
let ws=null,pollTimer=null;
function logLine(l){
  const t=l.timestamp?new Date(l.timestamp).toLocaleTimeString():'';
  return `<div class="log-line"><span class="log-time">${esc(t)}</span><span class="log-level log-level-${esc(l.level||'info')}">[${esc(l.phase||'')}]</span> ${esc(l.line||'')}</div>`;
}
function render(data){
  const c=data.cluster||data;
  document.getElementById('cName').textContent=c.name||id;
  document.getElementById('cMeta').textContent=`${c.master_count||1} master · ${c.worker_count||0} worker · ${esc(c.proxmox_url||'')}`;
  const b=document.getElementById('cStatus');
  b.className='badge badge-'+badgeFor(data.status);
  b.textContent=data.status;
  const bar=document.getElementById('cBar');
  bar.style.width=(data.progress||0)+'%';bar.textContent=(data.progress||0)+'%';
  document.getElementById('cProg').style.display=(data.status==='deploying'||data.status==='provisioning')?'':'none';
  const logs=data.logs||[];
  document.getElementById('logs').innerHTML=logs.length?logs.map(logLine).join(''):'<div class="text-muted">No logs yet. Click Deploy.</div>';
  const box=document.getElementById('logBox');box.scrollTop=box.scrollHeight;
  document.getElementById('btnDeploy').style.display=(data.status==='pending'||data.status==='error')?'':'none';
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
  poll();pollTimer=setInterval(poll,3000);connectWs();
});
