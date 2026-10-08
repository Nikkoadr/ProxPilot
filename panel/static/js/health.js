let step={key:false,ssh:false,prox:false};
function setStep(n,ok){
  step[n]=!!ok;
  const map={key:'st1',ssh:'st2',prox:'st3'};
  const el=document.getElementById(map[n]);
  if(el)el.className='badge mr-1 '+(ok?'badge-success':'badge-secondary');
  const done=Object.values(step).filter(Boolean).length;
  document.title=`Proxmox Panel — Health & SSH (${done}/3)`;
}
async function loadTools(){
  try{
    const t=await apiGet('/api/tools');
    const rt=t.runtime||{};
    const wslEl=document.getElementById('wslState');
    if(rt.primary==='local'){
      wslEl.innerHTML='<span class="badge badge-success">native</span> <span class="small text-muted">'+esc(rt.label||'panel jalan langsung di Linux')+'</span>';
      document.getElementById('wslDistros').textContent='Tool lokal yang dipakai — bagian WSL di bawah tidak relevan.';
    }else{
      wslEl.innerHTML=t.wsl?.available?'<span class="badge badge-success">available</span>':'<span class="badge badge-danger">unavailable</span>';
      document.getElementById('wslDistros').textContent=t.wsl?.distros||'-';
    }
    row('TfLocal',t.terraform?.local);row('TfWsl',t.terraform?.wsl);
    row('AnLocal',t.ansible?.local);row('AnWsl',t.ansible?.wsl);
    row('SshLocal',t.ssh?.local);row('SshWsl',t.ssh?.wsl);
    // Mode native (WSL2/VM Linux): baris WSL pasti FAIL ("unavailable") —
    // sembunyikan agar tidak dikira rusak. Tool lokal yang dipakai.
    const native=(rt.primary==='local');
    ['TfWsl','AnWsl','SshWsl'].forEach(id=>{
      const el=document.getElementById(id);
      if(el&&el.parentElement)el.parentElement.style.display=native?'none':'';
    });
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
  try{
    const r=await apiPost('/api/health/proxmox-test',body);
    out.textContent=JSON.stringify(r,null,2);
    setStep('prox',!!r.ok);
  }
  catch(e){out.textContent=String(e);setStep('prox',false);}
}
async function testSsh(){
  const out=document.getElementById('sshOut');out.textContent='testing ssh...';
  const body={ssh_host:document.getElementById('sHost').value.trim(),
    ssh_user:document.getElementById('sUser').value.trim()||'root',
    ssh_port:parseInt(document.getElementById('sPort').value,10)||22};
  try{
    const r=await apiPost('/api/ssh/test',body);
    out.textContent=JSON.stringify(r,null,2);
    setStep('ssh',!!r.ok);
  }
  catch(e){out.textContent=String(e);setStep('ssh',false);}
}
async function loadKey(){
  const st=document.getElementById('keyState'), pre=document.getElementById('pubKey');
  try{
    const k=await apiGet('/api/ssh/key');
    if(k.exists){st.className='badge badge-success';st.textContent='ada ('+(k.via||'')+')';pre.textContent=k.public_key;setStep('key',true);}
    else{st.className='badge badge-warning';st.textContent='belum ada';pre.textContent='(belum ada — klik Generate key)';setStep('key',false);}
  }catch(e){st.className='badge badge-danger';st.textContent='error';pre.textContent=String(e);}
}
async function genKey(){
  const out=document.getElementById('copyOut');out.textContent='generating...';
  try{await apiPost('/api/ssh/keygen',{});await loadKey();out.textContent='Key siap → lanjut langkah 2a.';}
  catch(e){out.textContent=String(e);}
}
async function copyId(){
  const out=document.getElementById('copyOut');out.textContent='copying key to server...';
  const v=id=>{const el=document.getElementById(id);return el?el.value.trim():'';};
  try{
    const r=await apiPost('/api/ssh/copy-id',{ssh_host:v('sHost'),ssh_user:v('sUser')||'root',
      ssh_port:parseInt(v('sPort'),10)||22,ssh_password:document.getElementById('cPass').value});
    document.getElementById('cPass').value='';
    out.textContent=(r.ok?'OK — key tersalin. Menjalankan Test SSH otomatis...\n':'GAGAL.\n')+JSON.stringify(r,null,2);
    if(r.ok)await testSsh();
  }catch(e){out.textContent=String(e);}
}
document.addEventListener('DOMContentLoaded',()=>{
  loadTools();setInterval(loadTools,10000);
  document.getElementById('btnProx').onclick=testProxmox;
  document.getElementById('btnSsh').onclick=testSsh;
  document.getElementById('btnKeygen').onclick=genKey;
  document.getElementById('btnCopyId').onclick=copyId;
  loadKey();
});
