async function loadDashboard(){
  try{
    const [summary,clusters,nodes,tools]=await Promise.all([
      apiGet('/api/realtime/summary'),apiGet('/api/clusters'),apiGet('/api/nodes'),apiGet('/api/tools')
    ]);
    setText('statRunning',summary.running);setText('statDeploying',summary.deploying);
    setText('statMasters',summary.master_nodes);setText('statWorkers',summary.worker_nodes);
    setText('statClusters',summary.clusters_total);setText('statNodes',Array.isArray(nodes)?nodes.length:0);
    setText('clock',new Date(summary.server_time).toLocaleString());
    renderClusters(clusters);renderNodes(nodes);renderTools(tools);
  }catch(e){ console.warn(e); }
}
function setText(id,v){const el=document.getElementById(id);if(el)el.textContent=v;}
function renderClusters(clusters){
  const tb=document.getElementById('clusterRows');
  if(!tb)return;
  tb.innerHTML=(clusters||[]).length?(clusters.map(clusterRow).join('')):'<tr><td colspan="5" class="text-center text-muted">No clusters yet — <a href="/new-cluster.html">create one</a>.</td></tr>';
}
function renderNodes(nodes){
  const tb=document.getElementById('nodeRows');
  if(!tb)return;
  tb.innerHTML=(nodes||[]).map(n=>{
    const live=n.live?' <span class="badge badge-success">live</span>':' <span class="badge badge-secondary">mock</span>';
    const mem=n.memory?Math.round(n.used_mem/1048576)+' / '+Math.round(n.memory/1048576)+' MB':'-';
    return `<tr><td><strong>${esc(n.name)}</strong>${live}</td><td>${esc(n.status)}</td><td>${(n.cpu*100).toFixed(1)}%</td><td>${mem}</td></tr>`;
  }).join('')||'<tr><td colspan="4" class="text-center text-muted">no nodes</td></tr>';
}
function renderTools(t){
  const el=document.getElementById('toolsRow');
  if(!el||!t)return;
  const wsl=t.wsl?.available;
  const card=(title,ok,sub)=>`<div class="col-md-3 mb-2"><div class="border rounded p-2">
    <div><span class="rounded-circle d-inline-block ${ok?'bg-success':'bg-danger'}" style="width:10px;height:10px"></span> <strong>${title}</strong></div>
    <div class="small text-muted">${esc(sub||'')}</div></div></div>`;
  el.innerHTML=
    card('WSL',!!wsl,(t.wsl?.distros||'').split('\n').slice(0,2).join(' · ').slice(0,80))+
    card('Terraform (WSL)',!!t.terraform?.wsl?.ok,t.terraform?.wsl?.output)+
    card('Ansible (WSL)',!!t.ansible?.wsl?.ok,t.ansible?.wsl?.output)+
    card('SSH (WSL)',!!t.ssh?.wsl?.ok,t.ssh?.wsl?.output);
}
document.addEventListener('DOMContentLoaded',()=>{loadDashboard();setInterval(loadDashboard,5000);});
