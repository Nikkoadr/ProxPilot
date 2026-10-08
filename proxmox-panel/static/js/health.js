async function loadTools(){
  try{
    const t=await apiGet('/api/tools');
    document.getElementById('wslState').innerHTML=t.wsl?.available?'<span class="badge badge-success">available</span>':'<span class="badge badge-danger">unavailable</span>';
    document.getElementById('wslDistros').textContent=t.wsl?.distros||'-';
    row('TfLocal',t.terraform?.local);row('TfWsl',t.terraform?.wsl);
    row('AnLocal',t.ansible?.local);row('AnWsl',t.ansible?.wsl);
    row('SshLocal',t.ssh?.local);row('SshWsl',t.ssh?.wsl);
  }catch(e){console.warn(e);}
}
function row(id,o){
  const el=document.getElementById(id);
  if(!el||!o)return;
  el.innerHTML=`<span class="badge badge-${o.ok?'success':'danger'}">${o.ok?'OK':'FAIL'}</span> <span class="small text-muted">${esc(o.output||'')} ${o.ms!=null?'· '+o.ms+'ms':''}</span>`;
}
async function testProxmox(){
  const out=document.getElementById('proxOut');out.textContent='testing...';
  const body={proxmox_url:document.getElementById('pUrl').value.trim(),
    proxmox_user:document.getElementById('pUser').value.trim()||'root@pam',
    token_id:document.getElementById('pTokenId').value.trim(),
    token_secret:document.getElementById('pTokenSecret').value,
    verify_tls:document.getElementById('pVerify').checked};
  try{out.textContent=JSON.stringify(await apiPost('/api/health/proxmox-test',body),null,2);}
  catch(e){out.textContent=String(e);}
}
async function testSsh(){
  const out=document.getElementById('sshOut');out.textContent='testing via WSL ssh...';
  const body={ssh_host:document.getElementById('sHost').value.trim(),
    ssh_user:document.getElementById('sUser').value.trim()||'root',
    ssh_port:parseInt(document.getElementById('sPort').value,10)||22};
  try{out.textContent=JSON.stringify(await apiPost('/api/ssh/test',body),null,2);}
  catch(e){out.textContent=String(e);}
}
document.addEventListener('DOMContentLoaded',()=>{
  loadTools();setInterval(loadTools,10000);
  document.getElementById('btnProx').onclick=testProxmox;
  document.getElementById('btnSsh').onclick=testSsh;
});
