/* Codificador GIF89a mínimo y fiable, sin worker ni dependencias.
   - Cuantización por popularidad a <=256 colores (5 bits/canal) + LUT 15-bit (nearest).
   - "LZW sin comprimir": tabla de color fija de 256 → minCodeSize=8, codeSize=9, con clear-codes
     periódicos para mantener el tamaño de código constante. Correcto y simple (archivos algo mayores).
   API: window.encodeGIF(frames, w, h, delayCs, opts) -> Uint8Array
     frames : array de ImageData o de Uint8ClampedArray RGBA (longitud w*h*4)
     delayCs: retardo por fotograma en centisegundos (1/100 s)
     opts   : { transparent:bool, loop:int (0 = bucle infinito) }
*/
(function(){
  'use strict';
  function buildPalette(frames, transparent){
    const counts=new Map();
    let total=0; for(const fr of frames){ const d=fr.data||fr; total+=d.length>>2; }
    const step=Math.max(1, Math.floor(total/120000))*4;   // muestreo para el histograma
    for(const fr of frames){ const d=fr.data||fr; for(let i=0;i<d.length;i+=step){ if(transparent && d[i+3]<128) continue; const key=((d[i]>>3)<<10)|((d[i+1]>>3)<<5)|(d[i+2]>>3); counts.set(key,(counts.get(key)||0)+1); } }
    const cap=transparent?255:256;
    const top=[...counts.entries()].sort((a,b)=>b[1]-a[1]).slice(0,cap);
    const pal=[]; if(transparent) pal.push([0,0,0]);   // índice 0 = transparente
    for(const e of top){ const key=e[0]; let r=((key>>10)&31)<<3, g=((key>>5)&31)<<3, b=(key&31)<<3; pal.push([r|(r>>5), g|(g>>5), b|(b>>5)]); }
    while(pal.length<2) pal.push([0,0,0]);
    return pal;
  }
  function buildLUT(pal, transparent){
    const lut=new Uint8Array(32768); const start=transparent?1:0;
    for(let key=0;key<32768;key++){ const r=((key>>10)&31)<<3, g=((key>>5)&31)<<3, b=(key&31)<<3; let best=start,bd=1e12; for(let p=start;p<pal.length;p++){ const dr=pal[p][0]-r,dg=pal[p][1]-g,db=pal[p][2]-b; const dist=dr*dr+dg*dg+db*db; if(dist<bd){ bd=dist; best=p; } } lut[key]=best; }
    return lut;
  }
  function lzwBlocks(out, idx){
    const minCode=8, clear=256, eoi=257, codeSize=9, maxCode=512;
    out.push(minCode);
    let block=[], cur=0, curBits=0;
    function flushBlock(){ if(block.length){ out.push(block.length); for(let i=0;i<block.length;i++) out.push(block[i]); block.length=0; } }
    function w(code){ cur|=code<<curBits; curBits+=codeSize; while(curBits>=8){ block.push(cur&0xff); cur>>=8; curBits-=8; if(block.length===255) flushBlock(); } }
    let next=clear+2; w(clear);
    for(let i=0;i<idx.length;i++){ w(idx[i]); next++; if(next>=maxCode-1){ w(clear); next=clear+2; } }
    w(eoi);
    if(curBits>0){ block.push(cur&0xff); if(block.length===255) flushBlock(); }
    flushBlock();
    out.push(0x00);
  }
  function encodeGIF(frames, w, h, delayCs, opts){
    opts=opts||{}; const transparent=!!opts.transparent; const loop=opts.loop==null?0:opts.loop;
    const pal=buildPalette(frames, transparent), lut=buildLUT(pal, transparent);
    const out=[]; const push=b=>out.push(b&0xff); const push2=n=>{ out.push(n&0xff); out.push((n>>8)&0xff); };
    'GIF89a'.split('').forEach(c=>push(c.charCodeAt(0)));
    push2(w); push2(h); push(0x80|7); push(0); push(0);          // GCT flag + tamaño 2^8=256, bg=0, aspect=0
    for(let i=0;i<256;i++){ const c=pal[i]||[0,0,0]; push(c[0]); push(c[1]); push(c[2]); }
    push(0x21); push(0xFF); push(0x0B); 'NETSCAPE2.0'.split('').forEach(c=>push(c.charCodeAt(0))); push(0x03); push(0x01); push2(loop); push(0x00);
    for(const fr of frames){ const d=fr.data||fr;
      push(0x21); push(0xF9); push(0x04); push(((transparent?2:1)<<2)|(transparent?0x01:0x00)); push2(delayCs); push(transparent?0:0); push(0x00);   // GCE: disposal=2 (restaurar al fondo) en transparente → cada frame limpia el anterior (sin acumular al girar)
      push(0x2C); push2(0); push2(0); push2(w); push2(h); push(0x00);                                                       // image descriptor
      const idx=new Uint8Array(w*h);
      for(let i=0,p=0;i<d.length;i+=4,p++){ if(transparent && d[i+3]<128) idx[p]=0; else idx[p]=lut[((d[i]>>3)<<10)|((d[i+1]>>3)<<5)|(d[i+2]>>3)]; }
      lzwBlocks(out, idx);
    }
    push(0x3B);
    return new Uint8Array(out);
  }
  window.encodeGIF=encodeGIF;
})();
