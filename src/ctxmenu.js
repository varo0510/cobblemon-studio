/* Menú contextual propio de Cobblemon Studio (clic derecho).
   Bloquea el menú nativo de HTML EN TODA LA APP. En campos de texto muestra un menú propio
   de cortar/copiar/pegar; en el resto, las opciones registradas o un fallback.
   Excepción: si otro handler ya hizo preventDefault (cuentagotas del lienzo, OrbitControls),
   no se interfiere.
   API global:
     ctxMenu.register(selector, build)  // build(el,event) -> [items]
     ctxMenu.fallback(build)            // menú cuando nada coincide (opcional)
     ctxMenu.show(x,y,items) / ctxMenu.close()
   item = { label, icon?, hint?, action?, danger?, disabled?, sep? }
*/
(function(){
  'use strict';
  var MENU=null;
  function close(){ if(!MENU) return; MENU.remove(); MENU=null;
    document.removeEventListener('pointerdown',onDown,true);
    document.removeEventListener('keydown',onKey,true);
    window.removeEventListener('blur',close); window.removeEventListener('resize',close);
    document.removeEventListener('scroll',close,true); }
  function onDown(e){ if(MENU && !MENU.contains(e.target)) close(); }
  function onKey(e){ if(e.key==='Escape'){ e.preventDefault(); close(); } }
  function show(x,y,items){
    close(); items=(items||[]).filter(Boolean); if(!items.length) return;
    var m=document.createElement('div'); m.className='ctxmenu';
    items.forEach(function(it){
      if(it.sep){ var s=document.createElement('div'); s.className='ctxsep'; m.appendChild(s); return; }
      var b=document.createElement('div'); b.className='ctxitem'+(it.danger?' danger':'')+(it.disabled?' off':'');
      var ic=document.createElement('span'); ic.className='cxi'; ic.textContent=it.icon||''; b.appendChild(ic);
      var lb=document.createElement('span'); lb.className='cxl'; lb.textContent=it.label||''; b.appendChild(lb);
      if(it.hint){ var hk=document.createElement('span'); hk.className='cxk'; hk.textContent=it.hint; b.appendChild(hk); }
      if(!it.disabled) b.addEventListener('click',function(){ close(); try{ it.action&&it.action(); }catch(err){ console.error(err); } });
      m.appendChild(b);
    });
    m.style.visibility='hidden'; document.body.appendChild(m);
    var w=m.offsetWidth, h=m.offsetHeight, vw=window.innerWidth, vh=window.innerHeight;
    m.style.left=Math.max(4,Math.min(x, vw-w-6))+'px';
    m.style.top=Math.max(4,Math.min(y, vh-h-6))+'px';
    m.style.visibility=''; MENU=m;
    document.addEventListener('pointerdown',onDown,true);
    document.addEventListener('keydown',onKey,true);
    window.addEventListener('blur',close); window.addEventListener('resize',close);
    document.addEventListener('scroll',close,true);
  }

  // ---- portapapeles (con respaldo a execCommand) ----
  function writeClip(text){
    try{ if(navigator.clipboard&&navigator.clipboard.writeText){ navigator.clipboard.writeText(text); return; } }catch(e){}
    try{ var ta=document.createElement('textarea'); ta.value=text; ta.style.position='fixed'; ta.style.opacity='0'; document.body.appendChild(ta); ta.select(); document.execCommand('copy'); document.body.removeChild(ta); }catch(e){}
  }
  function readClip(){ try{ if(navigator.clipboard&&navigator.clipboard.readText) return navigator.clipboard.readText().catch(function(){return '';}); }catch(e){} return Promise.resolve(''); }

  // ---- menú para campos de texto (input/textarea/contenteditable/CodeMirror) ----
  function textField(el){
    if(!el) return null;
    if(el.tagName==='TEXTAREA') return el;
    if(el.tagName==='INPUT' && /^(text|search|url|email|tel|password|number|)$/i.test(el.type||'')) return el;
    var ce=el.closest('[contenteditable=""],[contenteditable="true"],.CodeMirror'); if(ce) return ce;
    return null;
  }
  function editableMenu(el){
    var isInput=(el.tagName==='INPUT'||el.tagName==='TEXTAREA') && typeof el.selectionStart==='number';
    var hasSel=isInput ? (el.selectionEnd>el.selectionStart) : !!String(document.getSelection&&document.getSelection()||'').length;
    var ro=isInput && (el.readOnly||el.disabled);
    function copy(){ el.focus(); if(isInput){ var s=el.value.substring(el.selectionStart,el.selectionEnd); if(s) writeClip(s); } else { try{document.execCommand('copy');}catch(e){} } }
    function cut(){ if(ro) return; el.focus(); if(isInput){ var a=el.selectionStart,b=el.selectionEnd,s=el.value.substring(a,b); if(s){ writeClip(s); el.setRangeText('',a,b,'end'); el.dispatchEvent(new Event('input',{bubbles:true})); } } else { try{document.execCommand('cut');}catch(e){} } }
    function paste(){ if(ro) return; el.focus(); readClip().then(function(txt){ if(!txt) return; if(isInput){ var a=el.selectionStart,b=el.selectionEnd; el.setRangeText(txt,a,b,'end'); el.dispatchEvent(new Event('input',{bubbles:true})); } else { try{document.execCommand('insertText',false,txt);}catch(e){} } }); }
    function selectAll(){ el.focus(); if(isInput) el.select(); else { try{document.execCommand('selectAll');}catch(e){} } }
    return [
      {icon:'✂',label:'Cortar',hint:'Ctrl+X',disabled:ro||!hasSel,action:cut},
      {icon:'⧉',label:'Copiar',hint:'Ctrl+C',disabled:!hasSel,action:copy},
      {icon:'📋',label:'Pegar',hint:'Ctrl+V',disabled:ro,action:paste},
      {sep:true},
      {icon:'▦',label:'Seleccionar todo',hint:'Ctrl+A',action:selectAll},
    ];
  }

  var PROVIDERS=[], FALLBACK=null;
  function register(sel,build){ PROVIDERS.push({sel:sel,build:build}); }
  function fallback(build){ FALLBACK=build; }
  document.addEventListener('contextmenu',function(e){
    if(e.defaultPrevented) return;       // ya lo gestionó un handler propio (cuentagotas, OrbitControls)
    e.preventDefault();                  // bloquear SIEMPRE el menú nativo de HTML
    var tf=textField(e.target);
    if(tf){ show(e.clientX,e.clientY,editableMenu(tf)); return; }
    var items=null;
    for(var i=0;i<PROVIDERS.length && !items;i++){ var el=e.target.closest(PROVIDERS[i].sel); if(el){ var r=PROVIDERS[i].build(el,e); if(r&&r.length) items=r; } }
    if(!items && FALLBACK){ var f=FALLBACK(e); if(f&&f.length) items=f; }
    if(items) show(e.clientX,e.clientY,items);
  },false);
  window.ctxMenu={ show:show, register:register, fallback:fallback, close:close };
})();
