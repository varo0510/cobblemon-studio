/* Codificador APNG (PNG animado) — alfa REAL y reemplazo de fotograma por blend_op=SOURCE,
   así nunca se acumulan los frames (a diferencia del GIF transparente, cuyo disposal muchos
   visores ignoran). Usa CompressionStream('deflate') (zlib) disponible en WebView2/Chromium.
   API:  await window.encodeAPNG(frames, w, h, delayMs) -> Uint8Array
     frames : array de ImageData (o {data:Uint8ClampedArray} RGBA de longitud w*h*4)
     delayMs: retardo por fotograma en milisegundos
*/
(function(){
  'use strict';
  const CRCT=(()=>{ const t=new Uint32Array(256); for(let n=0;n<256;n++){ let c=n; for(let k=0;k<8;k++) c=(c&1)?(0xEDB88320^(c>>>1)):(c>>>1); t[n]=c>>>0; } return t; })();
  function crc32(bytes){ let c=0xFFFFFFFF; for(let i=0;i<bytes.length;i++) c=CRCT[(c^bytes[i])&0xff]^(c>>>8); return (c^0xFFFFFFFF)>>>0; }
  function chunk(type,data){ const cd=new Uint8Array(4+data.length); for(let i=0;i<4;i++) cd[i]=type.charCodeAt(i); cd.set(data,4);
    const crc=crc32(cd); const out=new Uint8Array(8+data.length+4); const dv=new DataView(out.buffer);
    dv.setUint32(0,data.length); out.set(cd,4); dv.setUint32(8+data.length,crc); return out; }
  function rawRGBA(im,w,h){ const d=im.data||im; const out=new Uint8Array(h*(1+w*4)); let o=0; for(let y=0;y<h;y++){ out[o++]=0; const row=y*w*4; out.set(d.subarray(row,row+w*4),o); o+=w*4; } return out; }
  async function deflate(u8){ const cs=new CompressionStream('deflate'); const wr=cs.writable.getWriter(); wr.write(u8); wr.close(); const ab=await new Response(cs.readable).arrayBuffer(); return new Uint8Array(ab); }
  async function encodeAPNG(frames,w,h,delayMs){
    if(typeof CompressionStream==='undefined') throw new Error('CompressionStream no disponible');
    const N=frames.length; const parts=[ new Uint8Array([137,80,78,71,13,10,26,10]) ];
    const ihdr=new Uint8Array(13); const iv=new DataView(ihdr.buffer); iv.setUint32(0,w); iv.setUint32(4,h); ihdr[8]=8; ihdr[9]=6; ihdr[10]=0; ihdr[11]=0; ihdr[12]=0; parts.push(chunk('IHDR',ihdr));   // 8 bit, RGBA
    const actl=new Uint8Array(8); const av=new DataView(actl.buffer); av.setUint32(0,N); av.setUint32(4,0); parts.push(chunk('acTL',actl));   // N frames, bucle infinito
    let seq=0; const dn=Math.max(1,Math.round(delayMs)), dd=1000;
    for(let i=0;i<N;i++){ const comp=await deflate(rawRGBA(frames[i],w,h));
      const f=new Uint8Array(26); const fv=new DataView(f.buffer); fv.setUint32(0,seq++); fv.setUint32(4,w); fv.setUint32(8,h); fv.setUint32(12,0); fv.setUint32(16,0); fv.setUint16(20,dn); fv.setUint16(22,dd); f[24]=1; f[25]=0;   // dispose=BACKGROUND, blend=SOURCE
      parts.push(chunk('fcTL',f));
      if(i===0){ parts.push(chunk('IDAT',comp)); }
      else { const fd=new Uint8Array(4+comp.length); new DataView(fd.buffer).setUint32(0,seq++); fd.set(comp,4); parts.push(chunk('fdAT',fd)); }
    }
    parts.push(chunk('IEND',new Uint8Array(0)));
    let len=0; for(const p of parts) len+=p.length; const out=new Uint8Array(len); let off=0; for(const p of parts){ out.set(p,off); off+=p.length; } return out;
  }
  window.encodeAPNG=encodeAPNG;
})();
