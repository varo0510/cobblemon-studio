// tauri-shim.js — puente frontend↔Rust en Tauri.
// Intercepta fetch('/api/...') y lo enruta a invoke(comando Rust). Los builders (JS)
// corren AQUÍ (en el WebView); Rust solo hace I/O/zip/fetch.
// CLAVE: resolvemos invoke EN CADA LLAMADA (no al cargar), por si __TAURI__ aún no está
// listo cuando este script corre en <head>. En navegador/Electron (sin invoke) NO toca nada.
(function () {
  function invokeFrom(w) {
    try { const T = w && w.__TAURI__; if (!T) return null; return (T.core && T.core.invoke) || T.invoke || (T.tauri && T.tauri.invoke) || null; } catch (e) { return null; }
  }
  // Busca invoke en esta ventana y, si estamos embebidos en un iframe, en parent/top
  // (Tauri no siempre inyecta __TAURI__ en subframes; el iframe es same-origin → podemos usar el del padre).
  function getInvoke() {
    return invokeFrom(window) || invokeFrom(window.parent) || invokeFrom(window.top);
  }
  const CSB = () => (window.CSB || {});
  const has = v => v !== undefined && v !== null && String(v).trim() !== '';

  function detectRootBone(geo) {
    try { const g = (geo['minecraft:geometry'] || [])[0]; const bones = (g && g.bones) || []; const r = bones.find(b => !b.parent); return (r && r.name) || (bones[0] && bones[0].name) || null; } catch (e) { return null; }
  }
  function renameAnimGroup(animJson, sp) {
    if (!animJson || !animJson.animations) return animJson;
    const out = {}; for (const k in animJson.animations) { const m = k.match(/^animation\.[^.]+\.(.+)$/); out[m ? ('animation.' + sp + '.' + m[1]) : k] = animJson.animations[k]; }
    return Object.assign({}, animJson, { animations: out });
  }

  async function doGenerate(invoke, b) {
    const B = CSB().BUILDERS || {};
    const files = [], errors = [];
    for (const it of (b.items || [])) {
      const fn = B[it.kind];
      if (!fn) { errors.push('(' + it.kind + ') tipo desconocido'); continue; }
      try { for (const f of fn(it)) files.push(f); } catch (e) { errors.push('(' + it.kind + ') ' + e.message); }
    }
    const root = (b.target && (b.target.path || ((b.target.parent || '') + '\\' + (b.target.name || '')))) || '';
    if (b.dryRun) return { dryRun: true, root, willWrite: files.length, exists: [], errors };
    return await invoke('write_pack', { root, files, rawFiles: b.rawFiles || [], overwrite: !!b.overwrite });
  }

  async function doCreatorBuild(invoke, b) {
    const C = CSB();
    const s = b.species || {}, spw = b.spawn || {}, m = b.model || {};
    const sp = C.spId(s.name); if (!sp) return { error: 'Falta el nombre del Pokémon' };
    const dex = C.intOr(s.dexNumber, 0);
    const folder = dex > 0 ? String(dex).padStart(4, '0') + '_' + sp : sp;
    let geo = null, texB64 = null, shinyB64 = null, anim = null;
    if (m.source === 'reuse') {
      if (!m.geoRel) return { error: 'Elige un modelo' };
      const r = await invoke('get_model', { geo: m.geoRel });
      geo = r.geo;
      if (r.texture) texB64 = String(r.texture).split(',').pop();
      if (r.shiny) shinyB64 = String(r.shiny).split(',').pop();
      anim = (r.animation && r.animation.animations) ? r.animation : null;
    } else {
      if (!m.geoText) return { error: 'Sube un modelo .geo.json' };
      geo = typeof m.geoText === 'string' ? JSON.parse(m.geoText) : m.geoText;
      if (m.textureB64) texB64 = String(m.textureB64).split(',').pop();
      if (m.shinyB64) shinyB64 = String(m.shinyB64).split(',').pop();
      if (m.animText) { try { anim = typeof m.animText === 'string' ? JSON.parse(m.animText) : m.animText; } catch (e) {} }
    }
    if (!geo) return { error: 'Modelo inválido' };
    const rootBone = detectRootBone(geo);
    try { geo['minecraft:geometry'][0].description.identifier = 'geometry.' + sp; } catch (e) {}
    if (anim) anim = renameAnimGroup(anim, sp);
    const bedrock = 'assets/cobblemon/bedrock/pokemon', texRoot = 'assets/cobblemon/textures/pokemon';
    const rawFiles = [{ rel: bedrock + '/models/' + folder + '/' + sp + '.geo.json', text: JSON.stringify(geo, null, 2) }];
    if (texB64) rawFiles.push({ rel: texRoot + '/' + folder + '/' + sp + '.png', base64: texB64 });
    if (shinyB64) rawFiles.push({ rel: texRoot + '/' + folder + '/' + sp + '_shiny.png', base64: shinyB64 });
    if (anim) rawFiles.push({ rel: bedrock + '/animations/' + folder + '/' + sp + '.animation.json', text: JSON.stringify(anim, null, 2) });
    const item = {
      kind: 'wiznewpokemon', namespace: 'cobblemon', species: sp, folder, model: sp, rootBone, shiny: !!shinyB64,
      name: s.name || C.cap(sp), dexNumber: dex, desc: s.desc,
      primaryType: s.primaryType, secondaryType: s.secondaryType, abilities: s.abilities,
      baseStats: s.baseStats, catchRate: s.catchRate, baseExperienceYield: s.baseExperienceYield,
      experienceGroup: s.experienceGroup, baseFriendship: s.baseFriendship, eggCycles: s.eggCycles,
      eggGroups: s.eggGroups, maleRatio: s.maleRatio, baseScale: s.baseScale, hitboxW: s.hitboxW, hitboxH: s.hitboxH,
      moves: s.moves, labels: s.labels, drops: s.drops, evolutions: s.evolutions,
    };
    if (spw && spw.enabled !== false && has(spw.biomes)) { item.spawnBiomes = spw.biomes; item.bucket = spw.bucket; item.level = spw.level; item.weight = spw.weight; }
    const B = C.BUILDERS || {};
    let files = [];
    try { files = B['wiznewpokemon'](item); } catch (e) { return { error: 'Builder: ' + e.message }; }
    const outName = (b.packName || ('cobblemon_' + sp));
    const root = b.outDir ? (b.outDir + '\\' + outName) : null;
    if (!root) return { error: 'No hay pack abierto (vuelve al Inicio y abre/crea uno).' };
    const desc = b.desc || ('Cobblemon: ' + (s.name || sp));
    const gen = await invoke('write_pack', { root, files, rawFiles, overwrite: true });
    const zips = await invoke('build_zips', { root, name: outName, datapackFormat: 48, resourceFormat: 34, desc });
    return { ok: true, root, zips, results: gen.results, species: sp, folder, rootBone };
  }

  async function route(invoke, url, opts) {
    const u = new URL(url, 'http://x'); const path = u.pathname; const q = u.searchParams;
    let body = {}; if (opts && opts.body) { try { body = JSON.parse(opts.body); } catch (e) {} }
    try {
      switch (path) {
        case '/api/defaults': return { datapacksDir: '', suggestedDir: '', packFormat: 48, resourceFormat: 34 };
        case '/api/pick-folder': return { path: await invoke('pick_folder', { start: body.start || '' }) };
        case '/api/project/create': return await invoke('project_create', { name: body.name, parent: body.parent, desc: body.desc }).then(r => ({ ok: true, root: r.root, name: r.name }));
        case '/api/project/open': return await invoke('project_open', { path: body.path }).then(r => ({ ok: true, root: r.root, name: r.name }));
        case '/api/packinfo': return await invoke('pack_info', { path: body.path });
        case '/api/preview/models': return await invoke('list_models');
        case '/api/preview/model': return await invoke('get_model', { geo: q.get('geo') });
        case '/api/preview/sample': { const r = await invoke('get_model', { geo: 'blockbench/pokemon/gen1/0006_charizard/charizard.geo.json' }); if (r && !r.error) r.name = 'Charizard (muestra)'; return r; }
        case '/api/preview/update': return await invoke('update_assets');
        case '/api/open': return await invoke('open_folder', { path: body.path });
        case '/api/packdetail': return await invoke('pack_detail', { path: body.path });
        case '/api/readfile': return await invoke('read_pack_file', { path: body.path, rel: body.rel });
        case '/api/readimage': return await invoke('read_pack_image', { path: body.path, rel: body.rel });
        case '/api/deletefile': return await invoke('delete_file', { path: body.path, rel: body.rel });
        case '/api/packverify': return await invoke('pack_verify', { path: body.path });
        case '/api/writefile': return await invoke('write_file', { path: body.path, rel: body.rel, text: body.text });
        case '/api/zip': return { zips: await invoke('build_zips', { root: body.root, name: body.name, datapackFormat: +body.datapackFormat || 48, resourceFormat: +body.resourceFormat || 34, desc: body.desc }) };
        case '/api/preview': { const B = CSB().BUILDERS || {}; const items = (body.items || []).map(it => { const fn = B[it.kind]; if (!fn) return { kind: it.kind, error: 'tipo desconocido' }; try { return { kind: it.kind, files: fn(it) }; } catch (e) { return { kind: it.kind, error: e.message }; } }); return { items }; }
        case '/api/generate': return await doGenerate(invoke, body);
        case '/api/creator/build': return await doCreatorBuild(invoke, body);
        default: return { error: 'ruta no soportada en Tauri: ' + path };
      }
    } catch (e) { return { error: String((e && e.message) || e) }; }
  }

  const _fetch = window.fetch.bind(window);
  const asResp = data => ({ ok: true, status: 200, json: async () => data, text: async () => JSON.stringify(data) });
  window.fetch = function (url, opts) {
    if (typeof url === 'string' && url.indexOf('/api/') === 0) {
      const invoke = getInvoke();
      if (invoke) return route(invoke, url, opts).then(asResp);
      // Sin invoke (ni en esta ventana ni en el padre): error claro, NO caer a red (Tauri no sirve HTTP).
      console.error('[Cobblemon Studio] /api sin puente Tauri (invoke no encontrado):', url);
      return Promise.resolve(asResp({ error: 'Puente Tauri no disponible (invoke no encontrado). Recarga la app.' }));
    }
    return _fetch(url, opts);
  };
  if (getInvoke()) console.log('[Cobblemon Studio] Tauri shim activo (invoke encontrado)');
  else console.log('[Cobblemon Studio] shim cargado; invoke se resolverá al llamar');
})();
