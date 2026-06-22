// ============================================================================
// Reproductor de animación Bedrock (.animation.json) para el visor de Cobblemon Studio.
// Soporta: canales rotation/position/scale como (a) arrays [x,y,z] de números o
// expresiones MoLang, (b) keyframes {tiempo:[..]} con interpolación lineal.
// MoLang: math.sin/cos (en GRADOS), math.* básicas, query.anim_time. Lo desconocido -> 0.
// La animación se aplica ENCIMA de la pose de reposo (g.userData.rest) que deja
// geoToThreeGroup, con el mismo espejo X de handedness (negar X,Y de rotación y X de posición).
// ============================================================================
'use strict';
(function () {
  const DEG = Math.PI / 180;
  const M = {
    sin: d => Math.sin(d * DEG), cos: d => Math.cos(d * DEG),
    abs: Math.abs, sqrt: Math.sqrt, pow: Math.pow, exp: Math.exp, ln: Math.log,
    floor: Math.floor, round: Math.round, ceil: Math.ceil, trunc: Math.trunc,
    mod: (a, b) => a - b * Math.floor(a / b),
    clamp: (v, a, b) => Math.min(b, Math.max(a, v)),
    lerp: (a, b, k) => a + (b - a) * k,
    min: Math.min, max: Math.max, atan2: (a, b) => Math.atan2(a, b) / DEG,
    random: a => a || 0, die_roll: () => 0, pi: Math.PI,
  };

  // MoLang string -> función(t). Si no compila o no valida, devuelve 0 constante.
  function compileExpr(src) {
    let s = String(src).toLowerCase();
    s = s.replace(/\b(query|q)\.anim_time\b/g, '(t)')
         .replace(/\b(query|q)\.life_time\b/g, '(t)')
         .replace(/\b(query|q)\.[a-z_0-9.]+/g, '(0)')       // otras queries -> 0
         .replace(/\b(variable|v|temp)\.[a-z_0-9.]+/g, '(0)') // variables -> 0
         .replace(/\bmath\.pi\b/g, '(' + Math.PI + ')')
         .replace(/\bmath\.([a-z_0-9]+)\s*\(/g, 'M.$1(');
    if (!/^[0-9eE.+\-*/%(),\sMa-z_]*$/.test(s) || /[=;\[\]{}`]/.test(s)) return () => 0;
    let fn;
    try { fn = new Function('t', 'M', 'return (' + (s || '0') + ');'); } catch (_) { return () => 0; }
    return t => { try { const r = fn(t, M); return (typeof r === 'number' && isFinite(r)) ? r : 0; } catch (_) { return 0; } };
  }

  // compila un valor de canal -> {const:bool, v:[fx,fy,fz]} donde cada f es número o función(t)
  const compileComp = c => (typeof c === 'string' ? compileExpr(c) : (typeof c === 'number' ? c : 0));
  const evalComp = (c, t) => (typeof c === 'function' ? c(t) : c);

  function compileVec(val) {
    if (Array.isArray(val)) return { kind: 'vec', v: [compileComp(val[0]), compileComp(val[1] ?? val[0]), compileComp(val[2] ?? 0)] };
    if (val && typeof val === 'object') {
      // keyframes: { "0.0":[..], "1.5":{post:[..]} , ... }
      const kf = Object.keys(val).map(k => {
        let raw = val[k]; if (raw && !Array.isArray(raw) && typeof raw === 'object') raw = raw.post || raw.pre || raw.vector || [0, 0, 0];
        return { t: parseFloat(k), v: Array.isArray(raw) ? [compileComp(raw[0]), compileComp(raw[1] ?? raw[0]), compileComp(raw[2] ?? 0)] : [compileComp(raw), 0, 0] };
      }).sort((a, b) => a.t - b.t);
      return { kind: 'kf', kf };
    }
    if (typeof val === 'number' || typeof val === 'string') { const c = compileComp(val); return { kind: 'vec', v: [c, c, c] }; }
    return null;
  }

  function evalVec(ch, t) {
    if (!ch) return null;
    if (ch.kind === 'vec') return [evalComp(ch.v[0], t), evalComp(ch.v[1], t), evalComp(ch.v[2], t)];
    const kf = ch.kf; if (!kf.length) return [0, 0, 0];
    if (t <= kf[0].t) return kf[0].v.map(c => evalComp(c, t));
    if (t >= kf[kf.length - 1].t) return kf[kf.length - 1].v.map(c => evalComp(c, t));
    let i = 0; while (i < kf.length - 1 && kf[i + 1].t < t) i++;
    const a = kf[i], b = kf[i + 1], k = (t - a.t) / ((b.t - a.t) || 1);
    return [0, 1, 2].map(j => { const av = evalComp(a.v[j], t), bv = evalComp(b.v[j], t); return av + (bv - av) * k; });
  }

  // elige la animación idle preferida del .animation.json
  function pickIdle(names) {
    return names.find(n => /ground_idle/i.test(n))
        || names.find(n => /(^|[._])idle($|[._])/i.test(n))
        || names.find(n => /idle/i.test(n)) || names[0];
  }

  // animationJson -> clip listo para reproducir (o null)
  function buildAnimClip(animationJson, preferName) {
    const anims = animationJson && animationJson.animations; if (!anims) return null;
    const names = Object.keys(anims); if (!names.length) return null;
    const name = (preferName && anims[preferName]) ? preferName : pickIdle(names);
    const a = anims[name]; if (!a) return null;
    const bonesSrc = a.bones || {}, bones = {};
    for (const bn in bonesSrc) {
      const b = bonesSrc[bn];
      bones[bn] = { rotation: compileVec(b.rotation), position: compileVec(b.position), scale: compileVec(b.scale) };
    }
    return { name, length: a.animation_length || 0, loop: a.loop !== false, bones };
  }

  function boneMap(root) {
    if (root.userData.boneMap) return root.userData.boneMap;
    const map = {}; root.traverse(o => { if (o.userData && o.userData.rest && o.name) map[o.name] = o; });
    root.userData.boneMap = map; return map;
  }

  function applyAnimClip(root, clip, time) {
    if (!root || !clip) return;
    const len = clip.length || 1;
    const t = clip.loop ? (time % len) : Math.min(time, len);
    const map = boneMap(root);
    for (const bn in clip.bones) {
      const g = map[bn]; if (!g) continue;
      const ch = clip.bones[bn], rest = g.userData.rest;
      const r = evalVec(ch.rotation, t);   // grados, espejo X,Y
      g.rotation.set(rest.rx + (r ? -r[0] * DEG : 0), rest.ry + (r ? -r[1] * DEG : 0), rest.rz + (r ? r[2] * DEG : 0));
      const p = evalVec(ch.position, t);   // unidades, espejo X
      g.position.set(rest.px + (p ? -p[0] : 0), rest.py + (p ? p[1] : 0), rest.pz + (p ? p[2] : 0));
      const s = evalVec(ch.scale, t);
      if (s) g.scale.set(s[0] || 1, s[1] || 1, s[2] || 1); else g.scale.set(1, 1, 1);
    }
  }

  // restaura todos los huesos a su pose de reposo (al apagar la animación)
  function resetToRest(root) {
    if (!root) return;
    root.traverse(o => { const r = o.userData && o.userData.rest; if (r) { o.position.set(r.px, r.py, r.pz); o.rotation.set(r.rx, r.ry, r.rz); o.scale.set(1, 1, 1); } });
  }

  window.buildAnimClip = buildAnimClip;
  window.applyAnimClip = applyAnimClip;
  window.resetToRest = resetToRest;
})();
