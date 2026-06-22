// Cobblemon Studio — backend Tauri (Rust). Solo "fontanería" (I/O, zip, fetch, diálogos);
// los ~40 builders de contenido siguen en JS y corren en el WebView (ver src/builders.js).
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use base64::Engine;
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::Manager;
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons};

// flag "hay cambios sin guardar" que el frontend mantiene al día; se consulta al cerrar la ventana
struct UnsavedFlag(AtomicBool);

const ASSETS_REPO: &str = "D:/Kyobble2/herramientas/cobblemon-assets"; // clon dev opcional
const GITLAB_BASE: &str = "https://gitlab.com/cable-mc/cobblemon-assets/-/raw/master/";
static INDEX_JSON: &str = include_str!("../../src/assets-index.json");

fn home_dir() -> PathBuf { PathBuf::from(std::env::var("USERPROFILE").unwrap_or_else(|_| ".".into())) }
fn cache_dir() -> PathBuf { home_dir().join(".cobblemon-studio").join("assets") }
fn index() -> Value { serde_json::from_str(INDEX_JSON).unwrap_or_else(|_| json!({"models": []})) }
fn b64(b: &[u8]) -> String { base64::engine::general_purpose::STANDARD.encode(b) }

fn slugify(s: &str) -> String {
    let mut out = String::new();
    for c in s.to_lowercase().chars() {
        if c.is_ascii_alphanumeric() || c == '_' { out.push(c); }
        else if !out.ends_with('_') { out.push('_'); }
    }
    out.trim_matches('_').to_string()
}

fn write_atomic(full: &Path, data: &[u8]) -> std::io::Result<()> {
    if let Some(p) = full.parent() { fs::create_dir_all(p)?; }
    let tmp = full.with_extension("cs_tmp");
    fs::write(&tmp, data)?;
    fs::rename(&tmp, full)
}

fn pack_mcmeta(format: i64, desc: &str) -> String {
    serde_json::to_string_pretty(&json!({"pack": {"pack_format": format, "description": desc}})).unwrap() + "\n"
}

fn urlencode(s: &str) -> String {
    s.chars().map(|c| if c.is_ascii_alphanumeric() || "-_.~".contains(c) { c.to_string() } else { format!("%{:02X}", c as u32) }).collect()
}

// asset: clon local -> caché -> GitLab (con timeout)
fn get_asset(rel: &str) -> Result<Vec<u8>, String> {
    let rel = rel.trim_start_matches('/');
    let local = Path::new(ASSETS_REPO).join(rel);
    if local.exists() { return fs::read(&local).map_err(|e| e.to_string()); }
    let cached = cache_dir().join(rel);
    if cached.exists() { return fs::read(&cached).map_err(|e| e.to_string()); }
    let url = format!("{}{}", GITLAB_BASE, rel.split('/').map(urlencode).collect::<Vec<_>>().join("/"));
    let resp = ureq::get(&url).timeout(std::time::Duration::from_secs(15)).call().map_err(|e| format!("GitLab: {}", e))?;
    let mut buf = Vec::new();
    resp.into_reader().read_to_end(&mut buf).map_err(|e| e.to_string())?;
    if let Some(p) = cached.parent() { let _ = fs::create_dir_all(p); }
    let _ = fs::write(&cached, &buf);
    Ok(buf)
}

// ---------- estructuras ----------
#[derive(Serialize)]
struct ProjectInfo { root: String, name: String }
#[derive(Deserialize)]
struct FileSpec { rel: String, obj: Value, #[serde(default)] merge: bool }
#[derive(Deserialize)]
struct RawFile { rel: String, #[serde(default)] text: Option<String>, #[serde(default)] base64: Option<String> }
#[derive(Serialize)]
struct WriteItem { rel: String, status: String, #[serde(skip_serializing_if = "Option::is_none")] error: Option<String> }

// ---------- comandos ----------
#[tauri::command]
fn pick_folder(app: tauri::AppHandle, start: Option<String>) -> Option<String> {
    let mut d = app.dialog().file();
    if let Some(s) = start { if !s.is_empty() && Path::new(&s).exists() { d = d.set_directory(PathBuf::from(s)); } }
    d.blocking_pick_folder().map(|p| p.to_string())
}

#[tauri::command]
fn project_create(parent: Option<String>, name: String, desc: Option<String>, pack_format: Option<i64>) -> Result<ProjectInfo, String> {
    let slug = slugify(&name);
    if slug.is_empty() { return Err("Ponle un nombre al pack".into()); }
    let parent = parent.filter(|s| !s.is_empty()).unwrap_or_else(|| home_dir().join("Desktop").join("Cobblemon_Packs").to_string_lossy().to_string());
    let root = Path::new(&parent).join(&slug);
    fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    let mcmeta = root.join("pack.mcmeta");
    if !mcmeta.exists() {
        fs::write(&mcmeta, pack_mcmeta(pack_format.unwrap_or(48), desc.as_deref().unwrap_or("Datapack Cobblemon"))).map_err(|e| e.to_string())?;
    }
    Ok(ProjectInfo { root: root.to_string_lossy().to_string(), name: slug })
}

#[tauri::command]
fn project_open(path: String) -> Result<ProjectInfo, String> {
    let p = Path::new(&path);
    if !p.exists() { return Err(format!("La carpeta del pack no existe: {}", path)); }
    let name = p.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    Ok(ProjectInfo { root: p.to_string_lossy().to_string(), name })
}

fn classify(rel: &str) -> Option<String> {
    if rel.contains("/species/") && rel.ends_with(".json")
        && !rel.contains("species_additions") && !rel.contains("species_features") && !rel.contains("species_feature_assignments") {
        return Some("species".into());
    }
    if let Some(rest) = rel.strip_prefix("data/") {
        let parts: Vec<&str> = rest.splitn(3, '/').collect();
        if parts.len() >= 2 {
            let t = match parts[1] {
                "species_additions" => Some("additions"), "species_features" => Some("features"),
                "species_feature_assignments" => Some("assignments"), "cosmetic_items" => Some("cosmetic"),
                "spawn_pool_world" => Some("spawns"), "spawn_detail_presets" => Some("presets"),
                "spawn_bait_effects" => Some("baits"), "dexes" => Some("dexes"), "dex_entries" => Some("dexentries"),
                "dex_additions" => Some("dexadd"), "dex_entry_additions" => Some("dexentryadd"),
                "berries" => Some("berries"), "marks" => Some("marks"), "fossils" => Some("fossils"),
                "pokerods" => Some("pokerods"), "seasonings" => Some("seasonings"), "natural_materials" => Some("naturalmat"),
                "recipe" | "recipes" => Some("recipes"), "tags" => Some("tags"), "loot_table" => Some("loot"),
                "global_species_features" => Some("globalfeat"), "pokemon_interactions" => Some("interactions"),
                "rideable_species" => Some("rideable"), _ => None,
            };
            if let Some(t) = t { return Some(t.into()); }
        }
    }
    if rel.contains("/resolvers/") { return Some("resolvers".into()); }
    if rel.contains("/posers/") { return Some("posers".into()); }
    if rel.contains("/models/") && (rel.ends_with(".geo.json") || rel.ends_with(".geo")) { return Some("models".into()); }
    if rel.contains("/animations/") { return Some("anims".into()); }
    if rel.contains("/textures/") && rel.to_lowercase().ends_with(".png") { return Some("textures".into()); }
    if rel.contains("/lang/") && rel.ends_with(".json") { return Some("lang".into()); }
    None
}

#[tauri::command]
fn pack_info(path: String) -> Value {
    let abs = Path::new(&path);
    if path.is_empty() || !abs.exists() { return json!({"exists": false}); }
    let mut counts: std::collections::BTreeMap<String, i64> = Default::default();
    let mut species: Vec<String> = Vec::new();
    for sub in ["data", "assets"] {
        let d = abs.join(sub);
        if !d.exists() { continue; }
        for entry in walkdir::WalkDir::new(&d).into_iter().filter_map(|e| e.ok()) {
            if !entry.file_type().is_file() { continue; }
            let rel = entry.path().strip_prefix(abs).unwrap_or(entry.path()).to_string_lossy().replace('\\', "/");
            if let Some(cat) = classify(&rel) {
                *counts.entry(cat.clone()).or_insert(0) += 1;
                if cat == "species" && species.len() < 60 {
                    if let Some(n) = entry.path().file_name() {
                        species.push(n.to_string_lossy().trim_end_matches(".json").to_string());
                    }
                }
            }
        }
    }
    species.sort();
    let total: i64 = counts.values().sum();
    json!({"exists": true, "path": abs.to_string_lossy(), "counts": counts, "lists": {"species": species}, "total": total})
}

fn cat_meta(cat: &str) -> (&'static str, &'static str, &'static str) {
    match cat {
        "species" => ("🐲", "Pokémon nuevos", "datapack"), "additions" => ("🧬", "Pokémon editados", "datapack"),
        "features" => ("✨", "Formas", "datapack"), "assignments" => ("🔗", "Asignaciones de forma", "datapack"),
        "cosmetic" => ("🎩", "Cosméticos", "datapack"), "spawns" => ("🌿", "Spawns", "datapack"),
        "presets" => ("🧩", "Presets de spawn", "datapack"), "baits" => ("🎣", "Cebos", "datapack"),
        "dexes" => ("📕", "Pokédex", "datapack"), "dexentries" => ("📄", "Entradas de dex", "datapack"),
        "dexadd" => ("➕", "Añadidos a dex", "datapack"), "dexentryadd" => ("📝", "Ediciones de dex", "datapack"),
        "berries" => ("🍒", "Bayas", "datapack"), "marks" => ("🎖️", "Marcas", "datapack"),
        "fossils" => ("🦴", "Fósiles", "datapack"), "pokerods" => ("🎏", "Cañas", "datapack"),
        "seasonings" => ("🧂", "Condimentos", "datapack"), "naturalmat" => ("🌾", "Materiales", "datapack"),
        "recipes" => ("⚒️", "Recetas", "datapack"), "tags" => ("🏷️", "Tags", "datapack"),
        "loot" => ("🎁", "Loot", "datapack"), "globalfeat" => ("📊", "Estad. globales", "datapack"),
        "interactions" => ("👋", "Interacciones", "datapack"), "rideable" => ("🐴", "Monturas", "datapack"),
        "resolvers" => ("🎨", "Apariencias (resolver)", "resource"), "posers" => ("🕹️", "Posers", "resource"),
        "models" => ("🧊", "Modelos 3D (.geo)", "resource"), "anims" => ("🎞️", "Animaciones", "resource"),
        "textures" => ("🖼️", "Texturas", "resource"), "lang" => ("🔤", "Textos (lang)", "resource"),
        _ => ("📦", "Otros", "datapack"),
    }
}

fn summarize_file(cat: &str, path: &Path) -> String {
    if matches!(cat, "textures" | "models" | "anims") { return String::new(); }
    let o: Value = match fs::read_to_string(path).ok().and_then(|s| serde_json::from_str(&s).ok()) { Some(v) => v, None => return String::new() };
    let s = |k: &str| o.get(k).and_then(|v| v.as_str()).unwrap_or("").to_string();
    let arr_len = |k: &str| o.get(k).and_then(|v| v.as_array()).map(|a| a.len()).unwrap_or(0);
    match cat {
        "species" => { let t = [s("primaryType"), s("secondaryType")].into_iter().filter(|x| !x.is_empty()).collect::<Vec<_>>().join(" / "); let d = o.get("nationalPokedexNumber").and_then(|v| v.as_i64()).map(|n| format!(" · #{}", n)).unwrap_or_default(); format!("{}{}", t, d) }
        "additions" => format!("edita {}", s("target")),
        "features" => { let mut r = s("type"); if let Some(ch) = o.get("choices").and_then(|v| v.as_array()) { r = format!("{}: {}", r, ch.iter().filter_map(|x| x.as_str()).collect::<Vec<_>>().join(",")); } r }
        "spawns" => o.get("spawns").and_then(|v| v.as_array()).map(|a| a.iter().map(|sp| format!("{} ({})", sp.get("pokemon").and_then(|v| v.as_str()).unwrap_or(""), sp.get("bucket").and_then(|v| v.as_str()).unwrap_or(""))).collect::<Vec<_>>().join(", ")).unwrap_or_default(),
        "dexentries" => s("speciesId"),
        "dexes" => format!("{} entradas", arr_len("entries")),
        "dexadd" => format!("→ {} ({})", s("dexId"), arr_len("entries")),
        "cosmetic" => o.get("pokemon").and_then(|v| v.as_array()).map(|a| a.iter().filter_map(|x| x.as_str()).collect::<Vec<_>>().join(", ")).unwrap_or_default(),
        "recipes" => format!("→ {}", o.get("result").and_then(|r| r.get("id").or_else(|| r.get("item"))).and_then(|v| v.as_str()).unwrap_or("")),
        "fossils" => format!("→ {}", s("result")),
        "marks" => format!("#{}", o.get("indexNumber").and_then(|v| v.as_i64()).map(|n| n.to_string()).unwrap_or_default()),
        "resolvers" => s("species"),
        "berries" => s("colour").to_lowercase(),
        "lang" => format!("{} textos", o.as_object().map(|m| m.len()).unwrap_or(0)),
        "tags" => format!("{} valores", arr_len("values")),
        _ => String::new(),
    }
}

#[tauri::command]
fn pack_detail(path: String) -> Value {
    let abs = Path::new(&path);
    if path.is_empty() || !abs.exists() { return json!({"exists": false}); }
    let mut groups: std::collections::BTreeMap<String, std::collections::BTreeMap<String, Vec<Value>>> = Default::default();
    groups.insert("datapack".into(), Default::default());
    groups.insert("resource".into(), Default::default());
    let mut total = 0i64;
    let mut labels = serde_json::Map::new();
    for sub in ["data", "assets"] {
        let d = abs.join(sub); if !d.exists() { continue; }
        for entry in walkdir::WalkDir::new(&d).into_iter().filter_map(|e| e.ok()) {
            if !entry.file_type().is_file() { continue; }
            let rel = entry.path().strip_prefix(abs).unwrap_or(entry.path()).to_string_lossy().replace('\\', "/");
            if let Some(cat) = classify(&rel) {
                let (emoji, label, grp) = cat_meta(&cat);
                labels.entry(cat.clone()).or_insert(json!([emoji, label, grp]));
                let name = entry.path().file_name().map(|n| n.to_string_lossy().trim_end_matches(".json").to_string()).unwrap_or_default();
                let summary = summarize_file(&cat, entry.path());
                groups.get_mut(grp).unwrap().entry(cat.clone()).or_default().push(json!({"rel": rel, "name": name, "summary": summary}));
                total += 1;
            }
        }
    }
    for g in groups.values_mut() { for items in g.values_mut() { items.sort_by(|a, b| a.get("name").and_then(|v| v.as_str()).unwrap_or("").cmp(b.get("name").and_then(|v| v.as_str()).unwrap_or("")) ); } }
    json!({"exists": true, "path": abs.to_string_lossy(), "groups": groups, "total": total, "labels": labels})
}

#[tauri::command]
fn read_pack_file(path: String, rel: String) -> Value {
    if rel.contains("..") { return json!({"error": "ruta inválida"}); }
    let full = Path::new(&path).join(rel.replace('\\', "/"));
    if !full.starts_with(Path::new(&path)) { return json!({"error": "fuera del pack"}); }
    if !full.is_file() { return json!({"error": "no encontrado"}); }
    let low = rel.to_lowercase();
    if low.ends_with(".png") || low.ends_with(".ogg") || low.ends_with(".nbt") {
        return json!({"binary": true, "size": fs::metadata(&full).map(|m| m.len()).unwrap_or(0)});
    }
    match fs::read_to_string(&full) { Ok(t) => json!({"text": t}), Err(e) => json!({"error": e.to_string()}) }
}

#[tauri::command]
fn read_pack_image(path: String, rel: String) -> Value {
    if rel.contains("..") { return json!({"error": "ruta inválida"}); }
    let full = Path::new(&path).join(rel.replace('\\', "/"));
    if !full.starts_with(Path::new(&path)) { return json!({"error": "fuera del pack"}); }
    if !full.is_file() { return json!({"error": "no encontrado"}); }
    if fs::metadata(&full).map(|m| m.len()).unwrap_or(0) > 8_000_000 { return json!({"error": "imagen demasiado grande"}); }
    match fs::read(&full) { Ok(b) => json!({"base64": b64(&b)}), Err(e) => json!({"error": e.to_string()}) }
}

#[tauri::command]
fn delete_file(path: String, rel: String) -> Value {
    if rel.contains("..") { return json!({"error": "ruta inválida"}); }
    let full = Path::new(&path).join(rel.replace('\\', "/"));
    if !full.exists() { return json!({"ok": true}); }
    match fs::remove_file(&full) { Ok(_) => json!({"ok": true}), Err(e) => json!({"error": e.to_string()}) }
}

#[tauri::command]
fn write_file(path: String, rel: String, text: String) -> Value {
    if rel.contains("..") { return json!({"error": "ruta inválida"}); }
    let full = Path::new(&path).join(rel.replace('\\', "/"));
    if !full.starts_with(Path::new(&path)) { return json!({"error": "fuera del pack"}); }
    match write_atomic(&full, text.as_bytes()) { Ok(_) => json!({"ok": true}), Err(e) => json!({"error": e.to_string()}) }
}

// ===== Verificador del pack (port fiel del packVerify de Electron) =====
fn vfy_add(issues: &mut Vec<Value>, level: &str, msg: String, file: &str) {
    issues.push(json!({"level": level, "msg": msg, "file": file}));
}
fn vfy_need(checklist: &mut Vec<Value>, seen: &mut std::collections::HashSet<String>, kind: &str, p: String, hint: &str) {
    if p.is_empty() || seen.contains(&p) { return; }
    seen.insert(p.clone());
    checklist.push(json!({"kind": kind, "path": p, "hint": hint}));
}
fn vfy_asset_path(r: &str) -> Option<String> {
    if !r.contains(':') { return None; }
    let i = r.find(':').unwrap();
    Some(format!("assets/{}/{}", &r[..i], &r[i + 1..]))
}
fn vfy_own(r: &str) -> bool {
    let ns = r.split(':').next().unwrap_or("");
    ns != "cobblemon" && ns != "minecraft" && ns != "c"
}
// ¿el poser anima con su propio nombre? aprox. del regex q\.bedrock[a-z_]*\(\s*'name'
fn vfy_poser_self_animates(txt: &str, name: &str) -> bool {
    if name.is_empty() { return false; }
    let pat = format!("'{}'", name);
    let mut from = 0;
    while let Some(rel) = txt[from..].find(&pat) {
        let idx = from + rel;
        let pre = txt[..idx].trim_end();
        if let Some(paren) = pre.strip_suffix('(') {
            let pt = paren.trim_end();
            if let Some(qpos) = pt.rfind("q.bedrock") {
                let tail = &pt[qpos + "q.bedrock".len()..];
                if tail.chars().all(|c| c.is_ascii_lowercase() || c == '_') { return true; }
            }
        }
        from = idx + pat.len();
    }
    false
}
// assets/<ns>/bedrock/pokemon/posers/<folder...>/<file>.json -> (ns, folder)
fn vfy_poser_path(rel: &str) -> (String, String) {
    let parts: Vec<&str> = rel.split('/').collect();
    if let Some(pi) = parts.iter().position(|&x| x == "posers") {
        let ns = if parts.len() > 1 { parts[1].to_string() } else { "cobblemon".into() };
        if parts.len() > pi + 2 { return (ns, parts[pi + 1..parts.len() - 1].join("/")); }
        return (ns, String::new());
    }
    ("cobblemon".into(), String::new())
}

#[tauri::command]
fn pack_verify(path: String) -> Value {
    use std::collections::{HashMap, HashSet};
    let abs = Path::new(&path);
    if path.is_empty() || !abs.exists() { return json!({"exists": false}); }

    // archivos de data/ y assets/
    struct F { rel: String, abs: std::path::PathBuf }
    let mut all: Vec<F> = Vec::new();
    for sub in ["data", "assets"] {
        let d = abs.join(sub);
        if !d.exists() { continue; }
        for e in walkdir::WalkDir::new(&d).into_iter().filter_map(|x| x.ok()) {
            if e.file_type().is_file() {
                if let Ok(r) = e.path().strip_prefix(abs) {
                    all.push(F { rel: r.to_string_lossy().replace('\\', "/"), abs: e.path().to_path_buf() });
                }
            }
        }
    }
    let relset: HashSet<String> = all.iter().map(|f| f.rel.to_lowercase()).collect();
    let has = |rel: &str| relset.contains(&rel.to_lowercase());
    let read = |p: &Path| -> Option<Value> {
        std::fs::read_to_string(p).ok().and_then(|s| serde_json::from_str::<Value>(&s).ok())
    };

    let mut issues: Vec<Value> = Vec::new();
    let mut checklist: Vec<Value> = Vec::new();
    let mut seen_check: HashSet<String> = HashSet::new();
    let mut poser_names: HashSet<String> = HashSet::new();
    let mut model_names: HashSet<String> = HashSet::new();
    let mut lang_keys: HashSet<String> = HashSet::new();
    let mut resolver_sp: HashSet<String> = HashSet::new();
    let mut species: Vec<(String, String)> = Vec::new();
    let mut marks: Vec<(Option<i64>, String, Option<String>, Option<String>)> = Vec::new();
    let mut anim_base: HashSet<String> = HashSet::new();
    let empty: Vec<Value> = Vec::new();

    // 1ª pasada: catalogar
    for f in &all {
        let rl = f.rel.to_lowercase();
        let fnm = rl.rsplit('/').next().unwrap_or("");
        if rl.contains("/posers/") && rl.ends_with(".json") {
            poser_names.insert(fnm.strip_suffix(".json").unwrap_or(fnm).to_string());
        }
        if rl.contains("/models/") && (rl.ends_with(".geo.json") || rl.ends_with(".geo")) {
            let b = fnm.strip_suffix(".geo.json").or_else(|| fnm.strip_suffix(".geo")).unwrap_or(fnm);
            model_names.insert(b.to_string());
        }
        if rl.contains("/animations/") && rl.ends_with(".animation.json") {
            anim_base.insert(fnm.strip_suffix(".animation.json").unwrap_or(fnm).to_string());
        }
        if rl.contains("/lang/") && rl.ends_with(".json") {
            match read(&f.abs) {
                Some(o) => { if let Some(obj) = o.as_object() { for k in obj.keys() { lang_keys.insert(k.to_lowercase()); } } }
                None => vfy_add(&mut issues, "error", "Archivo de textos (lang) con JSON inválido".into(), &f.rel),
            }
        }
        if rl.contains("/species/") && rl.ends_with(".json")
            && !rl.contains("species_additions") && !rl.contains("species_features") && !rl.contains("species_feature_assignments") {
            match read(&f.abs) {
                Some(o) => {
                    let name = o["name"].as_str().map(|s| s.to_string())
                        .unwrap_or_else(|| fnm.strip_suffix(".json").unwrap_or(fnm).to_string()).to_lowercase();
                    species.push((name, f.rel.clone()));
                }
                None => vfy_add(&mut issues, "error", "Un Pokémon (species) tiene el JSON inválido".into(), &f.rel),
            }
        }
        if rl.contains("/marks/") && rl.ends_with(".json") {
            if let Some(o) = read(&f.abs) {
                marks.push((o["indexNumber"].as_i64(), f.rel.clone(),
                    o["texture"].as_str().map(|s| s.to_string()), o["name"].as_str().map(|s| s.to_string())));
            }
        }
    }

    // 2ª pasada: resolvers (apariencias)
    for f in &all {
        let rl = f.rel.to_lowercase();
        if !(rl.contains("/resolvers/") && rl.ends_with(".json")) { continue; }
        let o = match read(&f.abs) { Some(o) => o, None => { vfy_add(&mut issues, "error", "Una apariencia (resolver) tiene el JSON inválido".into(), &f.rel); continue; } };
        if let Some(sp) = o["species"].as_str() { resolver_sp.insert(sp.to_lowercase()); }
        let rfolder = f.rel.split("/resolvers/").nth(1).and_then(|s| s.split('/').next()).unwrap_or("").to_string();
        let fp = if rfolder.is_empty() { String::new() } else { format!("{}/", rfolder) };
        for v in o["variations"].as_array().unwrap_or(&empty) {
            if let Some(t) = v["texture"].as_str() { if vfy_own(t) { if let Some(p) = vfy_asset_path(t) { if !has(&p) {
                vfy_add(&mut issues, "error", format!("Falta la textura: {} → el Pokémon se vería sin textura", t), &f.rel);
                vfy_need(&mut checklist, &mut seen_check, "Textura (.png)", p, "la imagen del Pokémon");
            } } } }
            for ly in v["layers"].as_array().unwrap_or(&empty) {
                if let Some(t) = ly["texture"].as_str() { if vfy_own(t) { if let Some(p) = vfy_asset_path(t) { if !has(&p) {
                    vfy_add(&mut issues, "warn", format!("Falta una capa de textura: {}", t), &f.rel);
                    vfy_need(&mut checklist, &mut seen_check, "Textura de capa (.png)", p, "capa emisiva/extra");
                } } } }
            }
            if let Some(ps) = v["poser"].as_str() { if vfy_own(ps) {
                let nm = ps.rsplit(|c| c == '/' || c == ':').next().unwrap_or(ps).to_lowercase();
                let pns = ps.split(':').next().unwrap_or("cobblemon");
                if !poser_names.contains(&nm) {
                    vfy_add(&mut issues, "warn", format!("El poser \"{}\" no se encuentra (faltarían animaciones)", ps), &f.rel);
                    vfy_need(&mut checklist, &mut seen_check, "Poser (.json)", format!("assets/{}/bedrock/pokemon/posers/{}{}.json", pns, fp, nm), "liga las animaciones a las poses");
                }
            } }
            if let Some(md) = v["model"].as_str() { if vfy_own(md) {
                let seg = md.rsplit(|c| c == '/' || c == ':').next().unwrap_or(md);
                let nm = if seg.to_lowercase().ends_with(".geo") { &seg[..seg.len() - 4] } else { seg };
                let pns = md.split(':').next().unwrap_or("cobblemon");
                if !model_names.contains(&nm.to_lowercase()) {
                    vfy_add(&mut issues, "warn", format!("El modelo \"{}\" no se encuentra (¿exportaste el .geo de Blockbench?)", md), &f.rel);
                    vfy_need(&mut checklist, &mut seen_check, "Modelo 3D (.geo.json)", format!("assets/{}/bedrock/pokemon/models/{}{}.geo.json", pns, fp, nm), "exportar de Blockbench");
                }
            } }
        }
    }

    // posers que esperan su propia animación
    for f in &all {
        let rl = f.rel.to_lowercase();
        if !(rl.contains("/posers/") && rl.ends_with(".json")) { continue; }
        let fnm = rl.rsplit('/').next().unwrap_or("");
        let self_name = fnm.strip_suffix(".json").unwrap_or(fnm).to_string();
        if anim_base.contains(&self_name) { continue; }
        let txt = std::fs::read_to_string(&f.abs).unwrap_or_default().to_lowercase();
        let needle: String = self_name.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '_').collect();
        if vfy_poser_self_animates(&txt, &needle) {
            let (pns, folder) = vfy_poser_path(&f.rel);
            let fp = if folder.is_empty() { String::new() } else { format!("{}/", folder) };
            vfy_add(&mut issues, "warn", format!("Faltan las animaciones de \"{}\" (.animation.json)", self_name), &f.rel);
            vfy_need(&mut checklist, &mut seen_check, "Animación (.animation.json)", format!("assets/{}/bedrock/pokemon/animations/{}{}.animation.json", pns, fp, self_name), "exportar de Blockbench");
        }
    }

    // species sin apariencia / sin nombre
    for (name, file) in &species {
        if !resolver_sp.iter().any(|r| r.rsplit(':').next().unwrap_or(r) == name) {
            vfy_add(&mut issues, "warn", format!("El Pokémon \"{}\" no tiene apariencia (resolver): saldrá como sustituto/invisible", name), file);
        }
        if !lang_keys.contains(&format!("cobblemon.species.{}.name", name)) {
            vfy_add(&mut issues, "warn", format!("El Pokémon \"{}\" no tiene nombre en el juego (falta lang)", name), file);
        }
    }

    // marcas: índices duplicados, textura y nombre
    let mut seen_idx: HashMap<i64, bool> = HashMap::new();
    for (idx, file, _t, _n) in &marks {
        if let Some(i) = idx {
            if seen_idx.contains_key(i) { vfy_add(&mut issues, "error", format!("Dos marcas usan el mismo número {} (colisionan)", i), file); }
            else { seen_idx.insert(*i, true); }
        }
    }
    for (_idx, file, texture, mname) in &marks {
        if let Some(t) = texture { if vfy_own(t) { if let Some(p) = vfy_asset_path(t) { if !has(&p) {
            vfy_add(&mut issues, "warn", format!("A la marca le falta su textura: {}", t), file);
        } } } }
        if let Some(n) = mname { if !lang_keys.contains(&format!("{}.title", n).to_lowercase()) {
            vfy_add(&mut issues, "warn", format!("La marca no tiene nombre en el juego (falta lang \"{}.title\")", n), file);
        } }
    }

    let err = issues.iter().filter(|i| i["level"].as_str() == Some("error")).count();
    let warn = issues.iter().filter(|i| i["level"].as_str() == Some("warn")).count();
    json!({"exists": true, "total": all.len(), "issues": issues, "checklist": checklist, "counts": {"error": err, "warn": warn}})
}

#[tauri::command]
fn list_models() -> Value {
    let idx = index();
    let empty = vec![];
    let models = idx.get("models").and_then(|m| m.as_array()).unwrap_or(&empty);
    let mapped: Vec<Value> = models.iter().map(|m| json!({
        "name": m.get("name"), "gen": m.get("gen"), "geo": m.get("geo"), "tex": m.get("tex"),
        "hasTex": m.get("tex").map_or(false, |t| !t.is_null())
    })).collect();
    json!({"models": mapped})
}

#[tauri::command]
fn get_model(geo: String) -> Result<Value, String> {
    let geo_bytes = get_asset(&geo)?;
    let geo_json: Value = serde_json::from_slice(&geo_bytes).map_err(|e| e.to_string())?;
    let idx = index();
    let empty = vec![];
    let models = idx.get("models").and_then(|m| m.as_array()).unwrap_or(&empty);
    let entry = models.iter().find(|m| m.get("geo").and_then(|g| g.as_str()) == Some(geo.as_str()));
    let tex = entry.and_then(|e| e.get("tex")).and_then(|t| t.as_str()).map(|s| s.to_string())
        .unwrap_or_else(|| geo.replace(".geo.json", ".png"));
    let mut texture = Value::Null;
    let mut shiny = Value::Null;
    if !tex.is_empty() {
        if let Ok(b) = get_asset(&tex) { texture = json!(format!("data:image/png;base64,{}", b64(&b))); }
        if let Ok(b) = get_asset(&tex.replace(".png", "_shiny.png")) { shiny = json!(format!("data:image/png;base64,{}", b64(&b))); }
    }
    let animation = get_asset(&geo.replace(".geo.json", ".animation.json")).ok()
        .and_then(|b| serde_json::from_slice::<Value>(&b).ok()).unwrap_or(Value::Null);
    let dir = geo.rsplit_once('/').map(|(d, _)| d).unwrap_or("");
    let variants: Vec<Value> = models.iter()
        .filter(|m| m.get("geo").and_then(|g| g.as_str()).map_or(false, |g| g.rsplit_once('/').map(|(d, _)| d).unwrap_or("") == dir))
        .map(|m| json!({"name": m.get("name"), "geo": m.get("geo")})).collect();
    let name = geo.rsplit('/').next().unwrap_or("").trim_end_matches(".geo.json").to_string();
    let has_shiny = !shiny.is_null();
    Ok(json!({"geo": geo_json, "texture": texture, "shiny": shiny, "hasShiny": has_shiny,
        "name": name, "variants": variants, "animation": animation, "geoRel": geo}))
}

#[tauri::command]
fn write_pack(root: String, files: Vec<FileSpec>, raw_files: Vec<RawFile>, overwrite: bool) -> Result<Value, String> {
    let root_p = Path::new(&root);
    fs::create_dir_all(root_p).map_err(|e| e.to_string())?;
    let mut results: Vec<WriteItem> = Vec::new();
    for f in files {
        if f.rel.contains("..") { results.push(WriteItem { rel: f.rel, status: "error".into(), error: Some("ruta inválida".into()) }); continue; }
        let full = root_p.join(f.rel.replace('\\', "/"));
        let r = (|| -> Result<String, String> {
            if let Some(p) = full.parent() { fs::create_dir_all(p).map_err(|e| e.to_string())?; }
            if f.merge && full.exists() {
                let prev: Value = fs::read_to_string(&full).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_else(|| json!({}));
                let mut merged = prev.as_object().cloned().unwrap_or_default();
                if let Some(o) = f.obj.as_object() { for (k, v) in o { merged.insert(k.clone(), v.clone()); } }
                let data = serde_json::to_string_pretty(&Value::Object(merged)).map_err(|e| e.to_string())? + "\n";
                write_atomic(&full, data.as_bytes()).map_err(|e| e.to_string())?;
                return Ok("merged".into());
            }
            if full.exists() && !overwrite { return Ok("skipped".into()); }
            let data = serde_json::to_string_pretty(&f.obj).map_err(|e| e.to_string())? + "\n";
            write_atomic(&full, data.as_bytes()).map_err(|e| e.to_string())?;
            Ok("written".into())
        })();
        match r {
            Ok(s) => results.push(WriteItem { rel: f.rel, status: s, error: None }),
            Err(e) => results.push(WriteItem { rel: f.rel, status: "error".into(), error: Some(e) }),
        }
    }
    for rf in raw_files {
        if rf.rel.contains("..") || !(rf.rel.starts_with("data/") || rf.rel.starts_with("assets/")) {
            results.push(WriteItem { rel: rf.rel, status: "error".into(), error: Some("debe ir bajo data/ o assets/".into()) }); continue;
        }
        let full = root_p.join(rf.rel.replace('\\', "/"));
        let r = (|| -> Result<String, String> {
            if let Some(p) = full.parent() { fs::create_dir_all(p).map_err(|e| e.to_string())?; }
            if full.exists() && !overwrite { return Ok("skipped".into()); }
            if let Some(b) = &rf.base64 {
                let raw = b.rsplit(',').next().unwrap_or(b);
                let bytes = base64::engine::general_purpose::STANDARD.decode(raw).map_err(|e| e.to_string())?;
                write_atomic(&full, &bytes).map_err(|e| e.to_string())?;
            } else {
                write_atomic(&full, rf.text.as_deref().unwrap_or("").as_bytes()).map_err(|e| e.to_string())?;
            }
            Ok("written".into())
        })();
        match r {
            Ok(s) => results.push(WriteItem { rel: rf.rel, status: s, error: None }),
            Err(e) => results.push(WriteItem { rel: rf.rel, status: "error".into(), error: Some(e) }),
        }
    }
    Ok(json!({"root": root, "results": results}))
}

#[tauri::command]
fn build_zips(root: String, name: Option<String>, datapack_format: Option<i64>, resource_format: Option<i64>, desc: Option<String>) -> Result<Value, String> {
    let root_p = Path::new(&root);
    if !root_p.exists() { return Err(format!("No existe la carpeta del pack: {}", root)); }
    let name = name.filter(|s| !s.is_empty()).unwrap_or_else(|| root_p.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "pack".into()));
    let parent = root_p.parent().unwrap_or(root_p);
    let mut out = Vec::new();
    let specs = [("data", "_datapack.zip", datapack_format.unwrap_or(48), "datapack"), ("assets", "_resourcepack.zip", resource_format.unwrap_or(34), "resourcepack")];
    for (sub, suffix, fmt, kind) in specs {
        let dir = root_p.join(sub);
        if !dir.exists() { continue; }
        let zip_path = parent.join(format!("{}{}", name, suffix));
        let file = fs::File::create(&zip_path).map_err(|e| e.to_string())?;
        let mut zw = zip::ZipWriter::new(file);
        let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        let mut count = 0u32;
        for entry in walkdir::WalkDir::new(&dir).into_iter().filter_map(|e| e.ok()) {
            if !entry.file_type().is_file() { continue; }
            let rel = entry.path().strip_prefix(root_p).unwrap_or(entry.path()).to_string_lossy().replace('\\', "/");
            zw.start_file(rel, opts).map_err(|e| e.to_string())?;
            let data = fs::read(entry.path()).map_err(|e| e.to_string())?;
            zw.write_all(&data).map_err(|e| e.to_string())?;
            count += 1;
        }
        zw.start_file("pack.mcmeta", opts).map_err(|e| e.to_string())?;
        zw.write_all(pack_mcmeta(fmt, desc.as_deref().unwrap_or("Cobblemon pack")).as_bytes()).map_err(|e| e.to_string())?;
        zw.finish().map_err(|e| e.to_string())?;
        let size_kb = fs::metadata(&zip_path).map(|m| m.len()).unwrap_or(0) / 1024;
        out.push(json!({"kind": kind, "file": zip_path.to_string_lossy(), "sizeKB": size_kb, "files": count}));
    }
    Ok(json!(out))
}

#[tauri::command]
fn open_folder(path: String) {
    let _ = std::process::Command::new("explorer").arg(path).spawn();
}

#[tauri::command]
fn update_assets() -> Value {
    if Path::new(ASSETS_REPO).join(".git").exists() {
        match std::process::Command::new("git").args(["-C", ASSETS_REPO, "pull"]).output() {
            Ok(o) => json!({"ok": o.status.success(), "msg": String::from_utf8_lossy(&o.stdout).to_string() + &String::from_utf8_lossy(&o.stderr)}),
            Err(e) => json!({"ok": false, "msg": e.to_string()}),
        }
    } else {
        json!({"ok": true, "msg": "modo bajo-demanda: los modelos se bajan al día desde GitLab"})
    }
}

// ===== AUTO-UPDATE (Tauri updater) =====
// Comprueba GitHub Releases; si hay versión nueva la descarga e instala (NSIS, sin UAC en
// instalación por-usuario). Errores (sin red / aún sin release) se devuelven como texto, NO rompen el arranque.
#[tauri::command]
async fn check_and_update(app: tauri::AppHandle) -> Result<String, String> {
    use tauri_plugin_updater::UpdaterExt;
    let updater = match app.updater() {
        Ok(u) => u,
        Err(e) => return Ok(format!("disabled:{e}")),
    };
    match updater.check().await {
        Ok(Some(update)) => {
            let ver = update.version.clone();
            match update.download_and_install(|_chunk, _total| {}, || {}).await {
                Ok(_) => Ok(format!("updated:{ver}")),
                Err(e) => Ok(format!("error:{e}")),
            }
        }
        Ok(None) => Ok("none".into()),
        Err(e) => Ok(format!("error:{e}")),
    }
}

#[tauri::command]
fn restart_app(app: tauri::AppHandle) {
    app.restart();
}

// el frontend avisa si hay trabajo sin guardar (pintura, ediciones del pack, asistentes…)
#[tauri::command]
fn set_unsaved(state: tauri::State<UnsavedFlag>, flag: bool) {
    state.0.store(flag, Ordering::Relaxed);
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        // (sin auto-abrir DevTools: la app arranca limpia)
        .manage(UnsavedFlag(AtomicBool::new(false)))
        // anti-pérdida de datos: si hay cambios sin guardar, confirmar antes de cerrar la ventana
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let dirty = window.state::<UnsavedFlag>().0.load(Ordering::Relaxed);
                if dirty {
                    api.prevent_close();
                    let w = window.clone();
                    window.dialog()
                        .message("Hay cambios sin guardar. Si cierras ahora se perderán.")
                        .title("Cambios sin guardar")
                        .buttons(MessageDialogButtons::OkCancelCustom("Cerrar igualmente".to_string(), "Cancelar".to_string()))
                        .show(move |ok| { if ok { let _ = w.destroy(); } });
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            pick_folder, project_create, project_open, pack_info, list_models,
            get_model, write_pack, build_zips, open_folder, update_assets,
            pack_detail, read_pack_file, read_pack_image, delete_file, pack_verify, write_file,
            check_and_update, restart_app, set_unsaved
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
