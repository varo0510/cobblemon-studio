window.CSB=(function(){var module={exports:{}};
// src/builders.js — Builders PUROS (input de formulario -> [{rel,obj}]) de Cobblemon Studio.
// Extraído de server.js SIN cambios de comportamiento; cubierto por test/golden.js.
// Fuente única de la lógica de construcción de contenido Cobblemon (1.7.x).
'use strict';

const D = 'data/cobblemon';

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------
const slug = (s) => String(s || '').toLowerCase().replace(/[^a-z0-9_]+/g, '_').replace(/^_+|_+$/g, '').replace(/_+/g, '_');
const list = (a) => (Array.isArray(a) ? a : String(a || '').split(',')).map((x) => String(x).trim()).filter(Boolean);
const species = (a) => list(a).map((x) => x.toLowerCase().replace(/^cobblemon:/, ''));
const numOr = (v, d) => { const n = parseFloat(v); return isNaN(n) ? d : n; };
const intOr = (v, d) => { const n = parseInt(v, 10); return isNaN(n) ? d : n; };
const clampOr = (v, min, max, d) => { const n = parseInt(v, 10); return isNaN(n) ? d : Math.max(min, Math.min(max, n)); };
const has = (v) => v !== undefined && v !== null && String(v).trim() !== '';
// id de especie SANEADO: quita namespace y cualquier cosa que no sea [a-z0-9_]
// (evita generar archivos tipo "tapu koko.json" o "type: null.json" -> ids invalidos)
const spId = (v) => String(v || '').toLowerCase().replace(/^.*:/, '').replace(/[^a-z0-9_]/g, '');

// condition/anticondition: solo incluye campos rellenados
function buildCondition(c) {
  if (!c) return undefined;
  const o = {};
  const biomes = list(c.biomes); if (biomes.length) o.biomes = biomes;
  const nb = list(c.neededNearbyBlocks); if (nb.length) o.neededNearbyBlocks = nb;
  const bb = list(c.neededBaseBlocks); if (bb.length) o.neededBaseBlocks = bb;
  const st = list(c.structures); if (st.length) o.structures = st;
  if (has(c.minSkyLight)) o.minSkyLight = clampOr(c.minSkyLight, 0, 15, 0);
  if (has(c.maxSkyLight)) o.maxSkyLight = clampOr(c.maxSkyLight, 0, 15, 15);
  if (has(c.minLight)) o.minLight = clampOr(c.minLight, 0, 15, 0);
  if (has(c.maxLight)) o.maxLight = clampOr(c.maxLight, 0, 15, 15);
  if (has(c.minY)) o.minY = clampOr(c.minY, -64, 320, -64);
  if (has(c.maxY)) o.maxY = clampOr(c.maxY, -64, 320, 320);
  if (o.minY !== undefined && o.maxY !== undefined && o.minY > o.maxY) { const t = o.minY; o.minY = o.maxY; o.maxY = t; }
  if (c.canSeeSky === true || c.canSeeSky === false) o.canSeeSky = c.canSeeSky;
  if (has(c.fluid)) o.fluid = c.fluid;
  if (has(c.timeRange)) o.timeRange = c.timeRange;
  if (c.isRaining === true || c.isRaining === false) o.isRaining = c.isRaining;
  if (c.isThundering === true || c.isThundering === false) o.isThundering = c.isThundering;
  if (has(c.moonPhase)) o.moonPhase = String(c.moonPhase);
  return Object.keys(o).length ? o : undefined;
}

// ---------------------------------------------------------------------------
// BUILDERS: datos del formulario -> [{rel, obj}] (esquemas verificados)
// ---------------------------------------------------------------------------

// 1) cosmetic_items
function buildCosmetic(it) {
  const pokemon = species(it.pokemon);
  const cosmeticItems = (it.entries || []).filter((e) => e.consumedItem).map((e) => ({
    consumedItem: e.consumedItem.includes(':') ? e.consumedItem : 'minecraft:' + e.consumedItem,
    aspects: list(e.aspects),
  }));
  if (!pokemon.length || !cosmeticItems.length) throw new Error('Cosmetic: faltan pokemon u objetos');
  const name = slug(it.fileName || cosmeticItems[0].consumedItem.split(':').pop());
  return [{ rel: `${D}/cosmetic_items/${name}.json`, obj: { pokemon, cosmeticItems } }];
}

// 2) spawn_pool_world  (CAMPO CORRECTO 1.7.3 = spawnablePositionType)
function buildSpawn(it) {
  const spawns = (it.spawns || []).filter((s) => s.pokemon).map((s, i) => {
    const id = String(s.id || (s.pokemon.split(' ')[0] + '-' + (i + 1)))
      .toLowerCase().replace(/[^a-z0-9_-]+/g, '-').replace(/^-+|-+$/g, '');
    const sp = {
      id, pokemon: s.pokemon.toLowerCase(),
      presets: list(s.presets).length ? list(s.presets) : ['natural'],
      type: 'pokemon',
      spawnablePositionType: s.spawnablePositionType || 'grounded',
      bucket: s.bucket || 'common',
      level: s.level || '5-32',
      weight: numOr(s.weight, 6.0),
    };
    const cond = buildCondition(s.condition); if (cond) sp.condition = cond;
    const anti = buildCondition(s.anticondition); if (anti) sp.anticondition = anti;
    if (s.weightMultiplier && has(s.weightMultiplier.multiplier)) {
      const wm = { multiplier: numOr(s.weightMultiplier.multiplier, 1) };
      const wc = buildCondition(s.weightMultiplier.condition); if (wc) wm.condition = wc;
      sp.weightMultiplier = wm;
    }
    return sp;
  });
  if (!spawns.length) throw new Error('Spawn: sin entradas');
  const name = slug(it.fileName || spawns[0].pokemon.split(' ')[0]);
  return [{ rel: `${D}/spawn_pool_world/${name}.json`, obj: { enabled: true, neededInstalledMods: [], neededUninstalledMods: [], spawns } }];
}

// 2b) spawn_detail_presets
function buildSpawnPreset(it) {
  const name = slug(it.name); if (!name) throw new Error('Preset: falta el nombre');
  const obj = {};
  const cond = buildCondition(it.condition); if (cond) obj.condition = cond;
  const anti = buildCondition(it.anticondition); if (anti) obj.anticondition = anti;
  if (!obj.condition) throw new Error('Preset: la condicion esta vacia');
  return [{ rel: `${D}/spawn_detail_presets/${name}.json`, obj }];
}

// 2c) spawn_bait_effects
function buildBait(it) {
  const item = it.item && it.item.includes(':') ? it.item : (it.item ? 'cobblemon:' + it.item : '');
  if (!item) throw new Error('Cebo: falta el item');
  const effects = (it.effects || []).filter((e) => e.type).map((e) => {
    const o = { type: e.type.includes(':') ? e.type : 'cobblemon:' + e.type, chance: numOr(e.chance, 1.0), value: numOr(e.value, 1.0) };
    if (has(e.subcategory)) o.subcategory = e.subcategory;
    return o;
  });
  const name = slug(it.fileName || item.split(':').pop());
  return [{ rel: `${D}/spawn_bait_effects/${name}.json`, obj: { item, effects } }];
}

// 3) species_features + assignment
function buildFeature(it) {
  const key = slug(it.key); if (!key) throw new Error('Feature: falta la clave');
  const feature = { keys: [key], type: it.type === 'choice' ? 'choice' : 'flag', isAspect: true };
  if (feature.type === 'flag') feature.default = false;
  else {
    const choices = list(it.choices);
    if (!choices.length) throw new Error('Forma "choice": añade al menos una opción (separadas por comas)');
    feature.default = it.default || choices[0] || 'random';
    feature.choices = choices;
    feature.aspectFormat = it.aspectFormat || `${key}-{{choice}}`;
  }
  const out = [{ rel: `${D}/species_features/${key}.json`, obj: feature }];
  const pokemon = species(it.pokemon);
  if (pokemon.length) out.push({ rel: `${D}/species_feature_assignments/${key}.json`, obj: { pokemon, features: [key] } });
  return out;
}

// evolutions: traduce los datos simplificados de la UI a la estructura real
function buildEvolutions(evos) {
  return (evos || []).filter((e) => e.result).map((e) => {
    const result = e.result.toLowerCase().trim();
    const idBase = (e.sourceName || 'mon') + '_' + result.replace(/\s+/g, '');
    const ev = { id: slug(e.id || idBase), result, consumeHeldItem: !!e.consumeHeldItem, learnableMoves: list(e.learnableMoves), requirements: [] };
    if (e.kind === 'stone') {
      ev.variant = 'item_interact';
      ev.requiredContext = e.item && e.item.includes(':') ? e.item : 'cobblemon:' + (e.item || 'fire_stone');
    } else if (e.kind === 'trade') {
      ev.variant = 'trade';
    } else if (e.kind === 'friendship') {
      ev.variant = 'level_up';
      ev.requirements.push({ variant: 'friendship', amount: intOr(e.amount, 160) });
    } else { // level
      ev.variant = 'level_up';
      ev.requirements.push({ variant: 'level', minLevel: intOr(e.minLevel, 16) });
    }
    // requisitos genéricos (válidos para cualquier tipo de evolución)
    if (has(e.timeRange)) ev.requirements.push({ variant: 'time_range', range: e.timeRange });
    if (has(e.biome)) ev.requirements.push({ variant: 'biome', biomeCondition: e.biome });
    if (has(e.heldItem)) ev.requirements.push({ variant: 'held_item', itemCondition: e.heldItem.includes(':') ? e.heldItem : 'cobblemon:' + e.heldItem });
    if (Array.isArray(e.extraRequirements)) e.extraRequirements.forEach((r) => { if (r && typeof r === 'object') ev.requirements.push(r); });
    return ev;
  });
}

function buildDrops(d) {
  if (!d || !d.enabled) return undefined;
  const entries = (d.entries || []).filter((x) => x.item).map((x) => {
    const o = { item: x.item.includes(':') ? x.item : 'minecraft:' + x.item };
    if (has(x.quantityRange)) o.quantityRange = x.quantityRange;
    if (has(x.percentage)) o.percentage = numOr(x.percentage, 100);
    return o;
  });
  if (!entries.length) return undefined;
  return { amount: intOr(d.amount, entries.length), entries };
}

// riding (montura) -> bloque para meter en un species_addition. Formato 1.7.3.
function buildRidingBlock(r) {
  if (!r || !r.enabled) return undefined;
  const STAT = ['ACCELERATION', 'JUMP', 'SKILL', 'SPEED', 'STAMINA'];
  const behaviours = {};
  (r.environments || []).forEach((env) => {
    if (!env.key) return;
    const b = { key: env.key, rideSounds: [], stats: {} };
    STAT.forEach((s) => { b.stats[s] = (env.stats && env.stats[s]) ? String(env.stats[s]) : '30-60'; });
    if (env.amb === 'LAND') { b.canJump = env.canJump !== false; b.canSprint = env.canSprint !== false; }
    behaviours[env.amb] = b;
  });
  if (!Object.keys(behaviours).length) return undefined;
  // poseOffsets: el SERVIDOR (colision/camara/posicion logica) SOLO usa esto; ignora
  // el "offset" de nivel superior del seat. Si poseOffsets queda vacio, getOffset()
  // devuelve Vec3.ZERO y el jinete se coloca en el centro de la hitbox (mal). Por eso
  // aplicamos el offset a TODAS las poses montables (STAND/WALK/HOVER/FLY/FLOAT/SWIM/GLIDE).
  // OJO: el RENDER del cliente (donde se VE el jugador) lo da el locator "seat_1" del
  // modelo .geo; sin ese punto en Blockbench el jinete se vera descolocado aunque la
  // logica sea correcta. Afinar numeros con /calculateseatpositions <pose> in-game.
  const POSES = ['STAND', 'WALK', 'HOVER', 'FLY', 'FLOAT', 'SWIM', 'GLIDE'];
  const so = r.seatOffset || {};
  const off = { x: numOr(so.x, 0), y: numOr(so.y, -1.0), z: numOr(so.z, 0) };
  const seat = { offset: { x: 0, y: 0, z: 0 }, poseOffsets: [{ offset: off, poseTypes: POSES }] };
  return { behaviours, seats: [seat] };
}

// 4) species_additions (forma + moves/evolutions/drops/preEvolution + riding)
function buildAddition(it) {
  let target = String(it.target || '').toLowerCase().trim();
  if (!target) throw new Error('Addition: falta el Pokemon');
  if (!target.includes(':')) target = 'cobblemon:' + target;
  const monName = target.split(':').pop();
  const obj = { target };
  const features = list(it.features); if (features.length) obj.features = features;

  // edits a la raiz del species
  const rootMoves = list(it.moves); if (rootMoves.length) obj.moves = rootMoves;
  if (has(it.preEvolution)) obj.preEvolution = it.preEvolution.toLowerCase().trim();
  (it.evolutions || []).forEach((e) => { e.sourceName = monName; });
  const evos = buildEvolutions(it.evolutions); if (evos.length) obj.evolutions = evos;
  const drops = buildDrops(it.drops); if (drops) obj.drops = drops;
  const riding = buildRidingBlock(it.riding); if (riding) obj.riding = riding;

  const out = [];
  let aspects = [];
  if (it.form && it.form.enabled) {
    const f = it.form;
    const form = { name: f.name || 'custom' };
    if (f.primaryType) form.primaryType = f.primaryType;
    if (f.secondaryType) form.secondaryType = f.secondaryType;
    const ab = list(f.abilities); if (ab.length) form.abilities = ab;
    if (f.baseStats && f.baseStats.enabled) {
      const b = f.baseStats;
      form.baseStats = { hp: intOr(b.hp, 50), attack: intOr(b.attack, 50), defence: intOr(b.defence, 50),
        special_attack: intOr(b.special_attack, 50), special_defence: intOr(b.special_defence, 50), speed: intOr(b.speed, 50) };
    }
    const fmoves = list(f.moves); if (fmoves.length) form.moves = fmoves;
    aspects = list(f.aspects); if (aspects.length) form.aspects = aspects;
    if (f.shoulderMountable) {
      form.shoulderMountable = true;
      const fx = (f.shoulderEffects || []).filter((e) => e.effect).map((e) => ({
        type: 'potion_effect',
        effect: e.effect.includes(':') ? e.effect : 'minecraft:' + e.effect,
        amplifier: intOr(e.amplifier, 0),
        ambient: e.ambient !== false,
        showParticles: e.showParticles === true,
        showIcon: e.showIcon !== false,
      }));
      if (fx.length) form.shoulderEffects = fx;
    }
    form.evolutions = [];
    obj.forms = [form];
  }

  const name = slug(it.fileName || monName);
  out.push({ rel: `${D}/species_additions/${name}.json`, obj });

  if (it.createFeature && aspects.length) {
    for (const a of aspects) {
      const k = a.toLowerCase().trim();
      out.push({ rel: `${D}/species_features/${k}.json`, obj: { keys: [k], type: 'flag', isAspect: true, default: false } });
      out.push({ rel: `${D}/species_feature_assignments/${k}.json`, obj: { pokemon: [monName], features: [k] } });
    }
  }
  return out;
}

// 5) Montura como species_addition independiente (riding)
function buildMount(it) {
  let target = String(it.target || it.pokemon || '').toLowerCase().trim();
  if (!target) throw new Error('Montura: falta el Pokemon');
  if (!target.includes(':')) target = 'cobblemon:' + target;
  const riding = buildRidingBlock({ enabled: true, environments: it.environments, seatOffset: it.seatOffset });
  if (!riding) throw new Error('Montura: elige al menos un ambiente (tierra/aire/agua)');
  const name = slug(it.fileName || target.split(':').pop());
  return [{ rel: `${D}/species_additions/${name}.json`, obj: { target, riding } }];
}

// 6) Pokedex
function buildDex(it) {
  const id = it.id && it.id.includes(':') ? it.id : 'cobblemon:' + slug(it.id || 'mi_dex');
  const obj = { type: 'cobblemon:simple_pokedex_def', id, sortOrder: intOr(it.sortOrder, 50), entries: list(it.entries) };
  return [{ rel: `${D}/dexes/${slug(id.split(':').pop())}.json`, obj }];
}
function buildDexEntry(it) {
  let sp = String(it.speciesId || '').toLowerCase().trim();
  if (!sp) throw new Error('DexEntry: falta speciesId');
  if (!sp.includes(':')) sp = 'cobblemon:' + sp;
  const id = it.id && it.id.includes(':') ? it.id : 'cobblemon:' + sp.split(':').pop();
  const forms = list(it.forms).length ? list(it.forms) : ['Normal'];
  const obj = { id, speciesId: sp, displayAspects: list(it.displayAspects), conditionAspects: [],
    forms: forms.map((f) => ({ displayForm: f, unlockForms: [f] })), variations: [] };
  const region = slug(it.region || 'custom');
  const out = [{ rel: `${D}/dex_entries/pokemon/${region}/${slug(sp.split(':').pop())}.json`, obj }];
  if (has(it.description)) out.push(...buildLang({ namespace: sp.split(':')[0], extra: [{ key: `cobblemon.species.${slug(sp.split(':').pop())}.desc`, value: it.description }] }));
  return out;
}
function buildDexAddition(it) {
  const dexId = it.dexId && it.dexId.includes(':') ? it.dexId : 'cobblemon:' + slug(it.dexId || 'national');
  const obj = { dexId, entries: list(it.entries) };
  return [{ rel: `${D}/dex_additions/${slug(it.fileName || dexId.split(':').pop())}.json`, obj }];
}

// 7) Objetos
function buildBerry(it) {
  const name = slug(it.name); if (!name) throw new Error('Baya: falta el nombre');
  const fl = {}; ['spicy', 'dry', 'sweet', 'bitter', 'sour'].forEach((k) => { if (has(it[k])) fl[k.toUpperCase()] = clampOr(it[k], 0, 30, 0); });
  const obj = {
    baseYield: { min: intOr(it.yieldMin, 2), max: intOr(it.yieldMax, 3) },
    preferredBiomeTags: list(it.preferredBiomeTags),
    growthTime: { min: intOr(it.growthMin, 36), max: intOr(it.growthMax, 44) },
    refreshRate: { min: intOr(it.refreshMin, 18), max: intOr(it.refreshMax, 22) },
    weight: numOr(it.weight, 20),
    colour: (it.colour || 'RED').toUpperCase(),
  };
  if (Object.keys(fl).length) obj.flavours = fl;
  return [{ rel: `${D}/berries/${name}.json`, obj }];
}
function buildPokerod(it) {
  let ball = String(it.pokeBallId || '').toLowerCase().trim();
  if (!ball) throw new Error('Caña: falta la pokeball');
  if (!ball.includes(':')) ball = 'cobblemon:' + ball;
  const obj = { pokeBallId: ball };
  if (has(it.lineColor)) obj.lineColor = it.lineColor;
  return [{ rel: `${D}/pokerods/${slug(it.fileName || ball.split(':').pop())}.json`, obj }];
}
function buildMark(it) {
  const name = slug(it.name); if (!name) throw new Error('Marca: falta el nombre');
  const ns = slug(it.namespace) || 'cobblemon';
  const obj = {
    name: `cobblemon.mark.${name}`, title: `cobblemon.mark.${name}.title`,
    description: `cobblemon.mark.${name}.desc`,
    texture: it.texture || `${ns}:textures/gui/mark/${name}.png`,
    indexNumber: intOr(it.indexNumber, 1000),
  };
  if (has(it.titleColor)) obj.titleColor = it.titleColor.replace('#', '');
  if (has(it.chance)) obj.chance = numOr(it.chance, 0.04);
  const out = [{ rel: `${D}/marks/${name}.json`, obj }];
  const extra = [];
  if (has(it.title)) extra.push({ key: `cobblemon.mark.${name}.title`, value: it.title });
  if (has(it.descText)) extra.push({ key: `cobblemon.mark.${name}.desc`, value: it.descText });
  if (extra.length) out.push(...buildLang({ namespace: ns, extra }));
  return out;
}
function buildFossil(it) {
  const result = String(it.result || '').toLowerCase().replace(/^cobblemon:/, '').trim();
  if (!result) throw new Error('Fosil: falta el resultado');
  const fossils = list(it.fossils).map((f) => f.includes(':') ? f : 'cobblemon:' + f);
  if (!fossils.length) throw new Error('Fosil: faltan los items de fosil');
  return [{ rel: `${D}/fossils/${slug(it.fileName || result)}.json`, obj: { result, fossils } }];
}

// ===========================================================================
//  FASE 1: más tipos de datapack
// ===========================================================================
// dex_entry_additions: añade formas/variaciones a una entrada de Pokédex existente
function buildDexEntryAddition(it) {
  let eid = String(it.entryId || '').toLowerCase().trim();
  if (!eid) throw new Error('DexEntryAddition: falta entryId');
  if (!eid.includes(':')) eid = 'cobblemon:' + eid;
  const forms = list(it.forms);
  if (!forms.length) throw new Error('DexEntryAddition: indica al menos una forma');
  const obj = { entryId: eid, forms: forms.map((f) => ({ displayForm: f, unlockForms: [f] })) };
  return [{ rel: `${D}/dex_entry_additions/${slug(it.fileName || eid.split(':').pop())}.json`, obj }];
}
// pokemon_interactions: usar un item sobre un Pokémon dispara efectos
function buildPokemonInteraction(it) {
  const target = String(it.target || '').toLowerCase().replace(/^.*:/, '').trim();
  if (!target) throw new Error('Interacción: falta el Pokémon');
  const effects = (it.effects || []).filter((e) => e.variant).map((e) => {
    const o = { variant: e.variant };
    if (e.variant === 'drop_item' && has(e.item)) o.item = e.item.includes(':') ? e.item : 'minecraft:' + e.item;
    if (e.variant === 'play_sound' && has(e.sound)) o.sound = e.sound.includes(':') ? e.sound : 'minecraft:' + e.sound;
    return o;
  });
  if (!effects.length) throw new Error('Interacción: añade al menos un efecto');
  // grouping: SOLO existen estos 6 en Cobblemon; cualquier otro id parsea pero nunca
  // se dispara (interacción muerta en silencio). Por eso validamos contra la lista real.
  const VALID_GROUPINGS = ['bone_meal', 'brush', 'bucket', 'glass_bottle', 'milking', 'shears'];
  const g = String(it.grouping || '').toLowerCase().replace(/^cobblemon:/, '').trim();
  if (!g) throw new Error('Interacción: elige un "disparador" (grouping): ' + VALID_GROUPINGS.join(', '));
  if (!VALID_GROUPINGS.includes(g)) throw new Error('Interacción: grouping inválido "' + g + '". Válidos: ' + VALID_GROUPINGS.join(', '));
  const inter = { grouping: 'cobblemon:' + g };
  if (has(it.cooldown)) inter.cooldown = String(it.cooldown);
  if (has(it.heldItem)) inter.requirements = [{ variant: 'owner_held_item', itemCondition: it.heldItem }];
  inter.effects = effects;
  const obj = { requirements: [{ variant: 'properties', target }], interactions: [inter] };
  return [{ rel: `${D}/pokemon_interactions/${slug(it.fileName || target)}.json`, obj }];
}
// global_species_features: estadística/feature aplicada a TODOS los Pokémon
function buildGlobalFeature(it) {
  const key = slug(it.key);
  if (!key) throw new Error('Global feature: falta la clave');
  const type = ['integer', 'flag', 'choice'].includes(it.type) ? it.type : 'flag';
  const obj = { type, keys: [key] };
  if (type === 'integer') { obj.default = intOr(it.default, 0); obj.min = intOr(it.min, 0); obj.max = intOr(it.max, 100); }
  else if (type === 'choice') { const ch = list(it.choices); if (!ch.length) throw new Error('Feature global "choice": añade al menos una opción'); obj.default = it.default || ch[0]; obj.choices = ch; }
  else obj.default = false;
  obj.visible = it.visible !== false;
  return [{ rel: `${D}/global_species_features/${key}.json`, obj }];
}
// natural_materials: valor de "content" por item (compostaje/alimentación). Archivo = ARRAY
function buildNaturalMaterial(it) {
  const entries = (it.entries || []).filter((e) => has(e.item) || has(e.tag)).map((e) => {
    const o = { content: intOr(e.content, 1) };
    if (has(e.tag)) o.tag = e.tag.startsWith('#') ? e.tag : '#' + e.tag;
    else o.item = e.item.includes(':') ? e.item : 'minecraft:' + e.item;
    if (has(e.returnItem)) o.returnItem = e.returnItem.includes(':') ? e.returnItem : 'minecraft:' + e.returnItem;
    return o;
  });
  if (!entries.length) throw new Error('Natural materials: añade items');
  return [{ rel: `${D}/natural_materials/${slug(it.fileName || 'custom')}.json`, obj: entries }];
}
// seasonings: condimento de cocina (ingrediente -> color + efectos)
function buildSeasoning(it) {
  const ing = has(it.ingredient) ? (it.ingredient.includes(':') ? it.ingredient : 'minecraft:' + it.ingredient) : '';
  if (!ing) throw new Error('Condimento: falta el ingrediente');
  const obj = { ingredient: ing };
  if (has(it.colour)) obj.colour = it.colour;
  const fx = (it.mobEffects || []).filter((e) => e.effect).map((e) => ({
    effect: e.effect.includes(':') ? e.effect : 'minecraft:' + e.effect,
    duration: intOr(e.duration, 60), amplifier: intOr(e.amplifier, 0), ambient: false, visible: true, showIcon: true,
  }));
  if (fx.length) obj.mobEffects = fx;
  return [{ rel: `${D}/seasonings/${slug(it.fileName || ing.split(':').pop())}.json`, obj }];
}
// tag de Minecraft (item/block/biome/...) bajo cualquier namespace
function buildTag(it) {
  const reg = slug(it.registry) || 'item';
  const ns = slug(it.namespace) || 'cobblemon';
  const name = slug(it.name);
  if (!name) throw new Error('Tag: falta el nombre');
  const obj = { replace: !!it.replace, values: list(it.values) };
  return [{ rel: `data/${ns}/tags/${reg}/${name}.json`, obj }];
}
// receta de crafteo vanilla (shapeless o shaped), formato MC 1.21
function buildRecipe(it) {
  const ns = slug(it.namespace) || 'cobblemon';
  const name = slug(it.name);
  if (!name) throw new Error('Receta: falta el nombre');
  const result = has(it.result) ? (it.result.includes(':') ? it.result : 'minecraft:' + it.result) : '';
  if (!result) throw new Error('Receta: falta el resultado');
  const count = intOr(it.count, 1);
  let obj;
  if (it.shaped) {
    // el patrón conserva espacios (huecos significativos): no usar list() que hace trim
    const pattern = (Array.isArray(it.pattern) ? it.pattern : String(it.pattern || '').split(',')).filter((r) => r && r.length);
    if (!pattern.length) throw new Error('Receta: falta el patrón');
    const key = {};
    (it.keys || []).filter((k) => has(k.char) && has(k.item)).forEach((k) => { key[String(k.char).trim()[0]] = { item: k.item.includes(':') ? k.item : 'minecraft:' + k.item }; });
    // toda letra usada en el patrón (salvo el hueco) debe tener su item definido
    const usados = new Set(pattern.join('').split('').filter((ch) => ch !== ' '));
    for (const ch of usados) if (!key[ch]) throw new Error('Receta: la letra "' + ch + '" del patrón no tiene item asignado');
    obj = { type: 'minecraft:crafting_shaped', pattern, key, result: { id: result, count } };
  } else {
    const ings = list(it.ingredients).map((i) => ({ item: i.includes(':') ? i : 'minecraft:' + i }));
    if (!ings.length) throw new Error('Receta: faltan ingredientes');
    obj = { type: 'minecraft:crafting_shapeless', ingredients: ings, result: { id: result, count } };
  }
  return [{ rel: `data/${ns}/recipe/${name}.json`, obj }];
}

// ===========================================================================
//  RESOURCE PACK (assets/) — el "pegamento" visual. NO genera arte:
//  el .geo.json y las animaciones salen de Blockbench; las texturas de un editor.
// ===========================================================================
const cap = (s) => s ? s.charAt(0).toUpperCase() + s.slice(1) : s;
// quita extensión .geo y namespace de un nombre de modelo
const modelRef = (ns, m, sp) => `${ns}:${(m ? String(m) : sp).replace(/^.*:/, '').replace(/\.geo(\.json)?$/i, '')}.geo`;

// 8) resolver base (Pokémon nuevo o forma propia)
function buildResolver(it) {
  const ns = slug(it.namespace) || 'cobblemon';
  const sp = spId(it.species);
  if (!sp) throw new Error('Resolver: falta la especie');
  const folder = (it.folder ? slug(it.folder) : '') || sp;
  const tex = `${ns}:textures/pokemon/${folder}`;
  const base = {
    aspects: [],
    poser: `${ns}:${it.poser ? slug(it.poser) : sp}`,
    model: modelRef(ns, it.model, sp),
    texture: `${tex}/${sp}.png`,
  };
  if (it.emissive) base.layers = [{ name: 'emissive', texture: `${tex}/${sp}_emissive.png`, emissive: true, translucent: false }];
  const variations = [base];
  if (it.shiny) {
    const sh = { aspects: ['shiny'], texture: `${tex}/${sp}_shiny.png` };
    if (it.emissive) sh.layers = [{ name: 'emissive', texture: `${tex}/${sp}_emissive_shiny.png`, emissive: true, translucent: false }];
    variations.push(sh);
  }
  return [{ rel: `assets/${ns}/bedrock/pokemon/resolvers/${folder}/0_${sp}_base.json`, obj: { species: `${ns}:${sp}`, order: 0, variations } }];
}

// 8b) resolver override (inyecta una variation en un Pokémon EXISTENTE sin tocar su base)
function buildResolverOverride(it) {
  const targetRaw = String(it.target || it.species || '').toLowerCase().trim();
  if (!targetRaw) throw new Error('Override: falta la especie destino');
  const ns = targetRaw.includes(':') ? targetRaw.split(':')[0] : 'cobblemon';
  const sp = targetRaw.split(':').pop();
  const folder = (it.folder ? slug(it.folder) : '') || sp;
  const order = intOr(it.order, 1) || 1;
  const aspects = list(it.aspects);
  if (!aspects.length) throw new Error('Override: indica al menos un aspecto');
  const suffix = slug(it.suffix || aspects[0]);
  const tex = `${ns}:textures/pokemon/${folder}`;
  const variation = { aspects };
  if (it.poser) variation.poser = `${ns}:${slug(it.poser)}`;
  if (it.model) variation.model = modelRef(ns, it.model, sp);
  variation.texture = it.texture ? (it.texture.includes(':') ? it.texture : `${ns}:${it.texture}`) : `${tex}/${sp}_${suffix}.png`;
  if (it.emissive) variation.layers = [{ name: 'emissive', texture: `${tex}/${sp}_${suffix}_emissive.png`, emissive: true, translucent: false }];
  return [{ rel: `assets/${ns}/bedrock/pokemon/resolvers/${folder}/${order}_${sp}_${suffix}.json`, obj: { species: `${ns}:${sp}`, order, variations: [variation] } }];
}

// 8c) poser estándar (liga las animaciones del .animation.json a cada pose)
function buildPoser(it) {
  const ns = slug(it.namespace) || 'cobblemon';
  const sp = spId(it.species);
  if (!sp) throw new Error('Poser: falta la especie');
  const folder = (it.folder ? slug(it.folder) : '') || sp;
  const root = it.rootBone ? String(it.rootBone).trim() : sp;
  const bd = (a) => `q.bedrock('${sp}', '${a}')`;
  const blink = `q.bedrock_quirk('${sp}', 'blink')`;
  const obj = {
    portraitScale: numOr(it.portraitScale, 2.0),
    portraitTranslation: [numOr(it.portraitX, 0), numOr(it.portraitY, 0), 0],
    profileScale: numOr(it.profileScale, 0.7),
    profileTranslation: [numOr(it.profileX, 0), numOr(it.profileY, 0.5), 0],
    rootBone: root,
    animations: { cry: `q.bedrock_stateful('${sp}', 'cry')`, recoil: `q.bedrock_stateful('${sp}', 'recoil')`, faint: `q.bedrock_stateful('${sp}', 'faint')` },
    poses: {
      standing: { poseTypes: ['STAND', 'NONE', 'PORTRAIT', 'PROFILE'], isBattle: false, animations: ["q.look('head_ai')", bd('ground_idle')], quirks: [blink] },
      walking: { poseTypes: ['WALK'], animations: ["q.look('head_ai')", bd('ground_walk')], quirks: [blink] },
      'battle-standing': { poseTypes: ['STAND'], isBattle: true, animations: ["q.look('head_ai')", bd('battle_idle')], quirks: [blink] },
      sleep: { poseTypes: ['SLEEP'], animations: [bd('sleep')] },
    },
  };
  if (it.shoulder) {
    obj.poses.shoulder_left = { poseTypes: ['SHOULDER_LEFT'], animations: ["q.look('head_ai')", bd('shoulder_left')], quirks: [blink], transformedParts: [{ part: 'body', position: [-6, 0, 0] }] };
    obj.poses.shoulder_right = { poseTypes: ['SHOULDER_RIGHT'], animations: ["q.look('head_ai')", bd('shoulder_right')], quirks: [blink], transformedParts: [{ part: 'body', position: [6, 0, 0] }] };
  }
  return [{ rel: `assets/${ns}/bedrock/pokemon/posers/${folder}/${sp}.json`, obj }];
}

// 8d) lang (nombre/descripción + claves extra). generate() lo MERGEA si ya existe.
function buildLang(it) {
  const ns = slug(it.namespace) || 'cobblemon';
  const lang = (it.lang ? slug(it.lang) : '') || 'en_us';
  const obj = {};
  const sp = spId(it.species);
  if (sp) {
    obj[`cobblemon.species.${sp}.name`] = it.name || cap(sp);
    if (has(it.desc)) obj[`cobblemon.species.${sp}.desc`] = it.desc;
  }
  (it.extra || []).filter((e) => e.key).forEach((e) => { obj[e.key] = e.value || ''; });
  if (!Object.keys(obj).length) throw new Error('Lang: nada que escribir (pon especie o claves)');
  return [{ rel: `assets/${ns}/lang/${lang}.json`, obj, merge: true }];
}

// 9) species (Pokémon NUEVO completo, fichero de datos)
function buildSpecies(it) {
  const ns = slug(it.namespace) || 'cobblemon';
  const sp = spId(it.species);
  if (!sp) throw new Error('Species: falta el nombre');
  const st = it.baseStats || {};
  const obj = {
    implemented: true,
    name: it.name || cap(sp),
    nationalPokedexNumber: intOr(it.dexNumber, 0),
    primaryType: (it.primaryType || 'normal').toLowerCase(),
  };
  if (has(it.secondaryType)) obj.secondaryType = it.secondaryType.toLowerCase();
  obj.maleRatio = numOr(it.maleRatio, 0.5);
  obj.catchRate = clampOr(it.catchRate, 1, 255, 45);
  obj.baseScale = numOr(it.baseScale, 1.0);
  obj.baseStats = { hp: intOr(st.hp, 50), attack: intOr(st.attack, 50), defence: intOr(st.defence, 50), special_attack: intOr(st.special_attack, 50), special_defence: intOr(st.special_defence, 50), speed: intOr(st.speed, 50) };
  obj.evYield = { hp: 0, attack: 0, defence: 0, special_attack: 0, special_defence: 0, speed: 0 };
  obj.experienceGroup = it.experienceGroup || 'medium_fast';
  obj.baseExperienceYield = intOr(it.baseExperienceYield, 100);
  obj.baseFriendship = intOr(it.baseFriendship, 50);
  obj.eggCycles = intOr(it.eggCycles, 20);
  obj.eggGroups = list(it.eggGroups).length ? list(it.eggGroups) : ['undiscovered'];
  obj.abilities = list(it.abilities).length ? list(it.abilities) : ['runaway'];
  obj.hitbox = { width: numOr(it.hitboxW, 0.6), height: numOr(it.hitboxH, 0.8), fixed: false };
  obj.behaviour = { moving: { walk: { walkSpeed: 0.3 } } };
  const drops = buildDrops(it.drops); obj.drops = drops || { amount: 1, entries: [] };
  obj.moves = list(it.moves).length ? list(it.moves) : ['1:tackle'];
  obj.labels = list(it.labels).length ? list(it.labels) : ['custom'];
  obj.aspects = [];
  obj.pokedex = [`cobblemon.species.${sp}.desc`];
  obj.evolutions = buildEvolutions((it.evolutions || []).map((e) => ({ ...e, sourceName: sp })));
  obj.forms = [];
  return [{ rel: `data/${ns}/species/custom/${sp}.json`, obj }];
}

// === ASISTENTES (wizards): orquestan varios builders desde un formulario ===
// Pokémon nuevo completo: species + resolver + poser + lang (+ spawn + dex)
function wizNewPokemon(it) {
  const ns = slug(it.namespace) || 'cobblemon';
  const sp = spId(it.species);
  if (!sp) throw new Error('Pokémon nuevo: falta el nombre');
  const folder = (it.folder ? slug(it.folder) : '') || (has(it.dexNumber) ? String(intOr(it.dexNumber, 0)).padStart(4, '0') + '_' + sp : sp);
  const out = [];
  out.push(...buildSpecies({ ...it, namespace: ns, species: sp }));
  out.push(...buildResolver({ namespace: ns, species: sp, folder, model: it.model, shiny: it.shiny !== false, emissive: it.emissive }));
  out.push(...buildPoser({ namespace: ns, species: sp, folder, rootBone: it.rootBone, shoulder: it.shoulder }));
  out.push(...buildLang({ namespace: ns, species: sp, name: it.name || cap(sp), desc: it.desc }));
  if (has(it.spawnBiomes)) out.push(...buildSpawn({ fileName: sp, spawns: [{ pokemon: sp, bucket: it.bucket || 'rare', spawnablePositionType: 'grounded', level: it.level || '5-30', weight: it.weight || '5', presets: 'natural', condition: { biomes: it.spawnBiomes } }] }));
  if (has(it.dexId)) { out.push(...buildDexEntry({ speciesId: `${ns}:${sp}`, region: it.region || 'custom', forms: 'Normal' })); out.push(...buildDexAddition({ dexId: it.dexId, entries: `${ns}:${sp}` })); }
  return out;
}
// Forma regional/variante de un Pokémon existente: feature + addition + resolver override (+ lang)
function wizRegional(it) {
  const target = String(it.target || '').toLowerCase().trim();
  if (!target) throw new Error('Forma regional: falta el Pokémon');
  const ns = target.includes(':') ? target.split(':')[0] : 'cobblemon';
  const sp = target.split(':').pop();
  const aspect = slug(it.aspect) || (has(it.region) ? slug(it.region) : 'regional');
  const folder = (it.folder ? slug(it.folder) : '') || sp;
  const out = [];
  out.push(...buildFeature({ key: aspect, type: 'flag', pokemon: sp }));
  out.push(...buildAddition({ target, fileName: sp + '_' + aspect, createFeature: false,
    form: { enabled: true, name: it.formName || aspect, primaryType: it.primaryType, secondaryType: it.secondaryType, abilities: it.abilities, aspects: aspect,
      baseStats: it.baseStats ? { enabled: true, ...it.baseStats } : { enabled: false }, shoulderMountable: false, shoulderEffects: [] } }));
  out.push(...buildResolverOverride({ target, folder, aspects: aspect, order: it.order || 1, suffix: aspect, model: it.model, poser: it.poser, emissive: it.emissive }));
  if (has(it.name)) out.push(...buildLang({ namespace: ns, extra: [{ key: `cobblemon.species.${sp}.aspect.${aspect}`, value: it.name }] }));
  return out;
}
// Reskin de un Pokémon existente: feature + resolver override con textura (hereda modelo/poser)
function wizReskin(it) {
  const target = String(it.target || '').toLowerCase().trim();
  if (!target) throw new Error('Reskin: falta el Pokémon');
  const sp = target.split(':').pop();
  const aspect = slug(it.aspect) || 'reskin';
  const folder = (it.folder ? slug(it.folder) : '') || sp;
  const out = [];
  out.push(...buildFeature({ key: aspect, type: 'flag', pokemon: sp }));
  out.push(...buildResolverOverride({ target, folder, aspects: aspect, order: it.order || 1, suffix: aspect, texture: it.texture, emissive: it.emissive }));
  return out;
}

// Cosmético por objeto: cosmetic_items + resolver override (la parte visual)
function wizCosmetic(it) {
  const target = String(it.target || '').toLowerCase().trim();
  if (!target) throw new Error('Cosmético: falta el Pokémon');
  const sp = target.split(':').pop();
  const item = has(it.item) ? (it.item.includes(':') ? it.item : 'minecraft:' + it.item) : '';
  if (!item) throw new Error('Cosmético: falta el objeto');
  const aspect = slug(it.aspect) || ('cosmetic_item-' + item.split(':').pop());
  const folder = (it.folder ? slug(it.folder) : '') || sp;
  const out = [];
  out.push(...buildCosmetic({ pokemon: sp, fileName: it.fileName || aspect, entries: [{ consumedItem: item, aspects: aspect }] }));
  out.push(...buildResolverOverride({ target, folder, aspects: aspect, order: it.order || 1, suffix: aspect, model: it.model, texture: it.texture, poser: it.poser, emissive: it.emissive }));
  return out;
}
// Poblar: spawn + entrada de Pokédex + añadir a una dex
function wizSpawnDex(it) {
  const ns = slug(it.namespace) || 'cobblemon';
  const mon = String(it.pokemon || '').toLowerCase().replace(/^.*:/, '').trim();
  if (!mon) throw new Error('Spawn+Pokédex: falta el Pokémon');
  const out = [];
  out.push(...buildSpawn({ fileName: mon, spawns: [{ pokemon: mon, bucket: it.bucket || 'common', spawnablePositionType: it.pos || 'grounded', level: it.level || '5-30', weight: it.weight || '6', presets: 'natural',
    condition: { biomes: it.biomes, timeRange: it.timeRange, minLight: it.minLight, maxLight: it.maxLight, minSkyLight: it.minSkyLight, maxSkyLight: it.maxSkyLight, minY: it.minY, maxY: it.maxY, canSeeSky: it.canSeeSky, moonPhase: it.moonPhase, isRaining: it.isRaining, isThundering: it.isThundering, fluid: it.fluid, structures: it.structures, neededNearbyBlocks: it.neededNearbyBlocks } }] }));
  if (has(it.dexId)) {
    out.push(...buildDexEntry({ speciesId: ns + ':' + mon, region: it.region || 'custom', forms: 'Normal' }));
    out.push(...buildDexAddition({ dexId: it.dexId, entries: ns + ':' + mon }));
  }
  return out;
}
// Evolución: species_addition con la evolución (+ preEvolution en el resultado)
function wizEvolution(it) {
  const target = String(it.target || '').toLowerCase().trim();
  if (!target) throw new Error('Evolución: falta el Pokémon origen');
  const ns = target.includes(':') ? target.split(':')[0] : 'cobblemon';
  const sp = target.split(':').pop();
  const result = String(it.result || '').toLowerCase().replace(/^.*:/, '').trim();
  if (!result) throw new Error('Evolución: falta el resultado');
  const out = [];
  out.push(...buildAddition({ target, fileName: sp + '_evo', createFeature: false,
    evolutions: [{ kind: it.evoKind || 'level', result, minLevel: it.minLevel, item: it.item, amount: it.amount, timeRange: it.timeRange,
      biome: it.biome, heldItem: it.heldItem, consumeHeldItem: it.consumeHeldItem, extraRequirements: it.extraRequirements, learnableMoves: it.learnableMoves }],
    form: { enabled: false } }));
  if (it.setPreEvo) out.push(...buildAddition({ target: ns + ':' + result, fileName: result + '_preevo', createFeature: false, preEvolution: sp, form: { enabled: false } }));
  return out;
}

const BUILDERS = {
  cosmetic: buildCosmetic, spawn: buildSpawn, spawnpreset: buildSpawnPreset, bait: buildBait,
  feature: buildFeature, addition: buildAddition, mount: buildMount,
  dex: buildDex, dexentry: buildDexEntry, dexaddition: buildDexAddition,
  berry: buildBerry, pokerod: buildPokerod, mark: buildMark, fossil: buildFossil,
  resolver: buildResolver, resolveroverride: buildResolverOverride, poser: buildPoser, lang: buildLang,
  species: buildSpecies, wiznewpokemon: wizNewPokemon, wizregional: wizRegional, wizreskin: wizReskin,
  wizcosmetic: wizCosmetic, wizspawndex: wizSpawnDex, wizevolution: wizEvolution,
  dexentryaddition: buildDexEntryAddition, pokemoninteraction: buildPokemonInteraction, globalfeature: buildGlobalFeature,
  naturalmaterial: buildNaturalMaterial, seasoning: buildSeasoning, tag: buildTag, recipe: buildRecipe,
};

module.exports = { D, slug, list, species, numOr, intOr, clampOr, has, spId, cap, modelRef, BUILDERS };

;return module.exports;})();
