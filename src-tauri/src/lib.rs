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
    let mut tmp_os = full.as_os_str().to_owned();   // append (no with_extension): único aunque el destino ya acabe en .cs_tmp
    tmp_os.push(".cs_tmp");
    let tmp = PathBuf::from(tmp_os);
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
    // species (Pokémon nuevo) = SOLO datapack: data/<ns>/species/...  (el assets/.../bedrock/species/ son RESOLVERS, no species)
    if rel.starts_with("data/") && rel.contains("/species/") && rel.ends_with(".json")
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
    // resolvers (apariencias): carpeta antigua /resolvers/ o la moderna de Cobblemon assets/<ns>/bedrock/species/...
    if rel.contains("/resolvers/") { return Some("resolvers".into()); }
    if rel.starts_with("assets/") && rel.contains("/species/") && rel.ends_with(".json") { return Some("resolvers".into()); }
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
    if !full.starts_with(Path::new(&path)) { return json!({"error": "fuera del pack"}); }
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
        if rl.starts_with("data/") && rl.contains("/species/") && rl.ends_with(".json")
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
        let is_resolver = rl.ends_with(".json") && (rl.contains("/resolvers/") || (rl.starts_with("assets/") && rl.contains("/species/")));
        if !is_resolver { continue; }
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

// ===================== DOCTOR DEL PACK =====================
// pack_doctor = diagnóstico integral READ-ONLY (calcula previews sin tocar disco).
// doctor_apply_fixes = aplica una lista de arreglos ya confirmados, con backup + re-validación.
const POKE_TYPES: [&str; 18] = ["normal","fire","water","grass","electric","ice","fighting","poison","ground","flying","psychic","bug","rock","ghost","dragon","dark","steel","fairy"];

fn doctor_excluded(rel: &str) -> bool {
    rel.starts_with("_backup_doctor/") || rel.contains("/_backup_doctor/") || rel.ends_with(".cs_tmp")
}
fn title_case(s: &str) -> String {
    s.split(|c| c == '_' || c == ' ' || c == '-').filter(|w| !w.is_empty())
        .map(|w| { let mut ch = w.chars(); match ch.next() { Some(f) => f.to_uppercase().collect::<String>() + &ch.as_str().to_lowercase(), None => String::new() } })
        .collect::<Vec<_>>().join(" ")
}
fn valid_resource_id(s: &str) -> bool {
    if s.is_empty() { return false; }
    let (ns, path) = match s.split_once(':') { Some((a, b)) => (a, b), None => ("minecraft", s) };
    let ok = |c: char| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '.' || c == '-';
    !ns.is_empty() && ns.chars().all(ok) && !path.is_empty() && path.chars().all(|c| ok(c) || c == '/')
}
fn dfind(out: &mut Vec<Value>, id: &str, cat: &str, level: &str, title: &str, msg: String, file: &str, safety: &str, fix: Value, suggestion: &str) {
    let mut o = serde_json::Map::new();
    o.insert("id".into(), json!(id)); o.insert("category".into(), json!(cat)); o.insert("level".into(), json!(level));
    o.insert("title".into(), json!(title)); o.insert("msg".into(), json!(msg)); o.insert("file".into(), json!(file));
    o.insert("safety".into(), json!(safety));
    if !fix.is_null() { o.insert("fix".into(), fix); }
    if !suggestion.is_empty() { o.insert("suggestion".into(), json!(suggestion)); }
    out.push(Value::Object(o));
}
fn read_head(p: &Path, n: usize) -> Vec<u8> {   // lee solo los primeros n bytes (BOM/codificación sin leer el archivo entero)
    let mut f = match fs::File::open(p) { Ok(f) => f, Err(_) => return Vec::new() };
    let mut buf = vec![0u8; n];
    let r = f.read(&mut buf).unwrap_or(0);
    buf.truncate(r); buf
}
// Minecraft usa GSON tolerante: acepta comentarios // y /* */ y comas finales. Replicamos esa tolerancia
// para NO marcar como "roto" un JSON que el juego sí carga (p.ej. fonts con comentarios). Respeta las cadenas.
fn strip_jsonc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut it = s.chars().peekable();
    let (mut in_str, mut esc) = (false, false);
    while let Some(c) = it.next() {
        if in_str { out.push(c); if esc { esc = false; } else if c == '\\' { esc = true; } else if c == '"' { in_str = false; } continue; }
        if c == '"' { in_str = true; out.push(c); continue; }
        if c == '/' {
            match it.peek() {
                Some('/') => { while let Some(&n) = it.peek() { if n == '\n' { break; } it.next(); } continue; }
                Some('*') => { it.next(); let mut prev = ' '; while let Some(n) = it.next() { if prev == '*' && n == '/' { break; } prev = n; } continue; }
                _ => {}
            }
        }
        out.push(c);
    }
    out
}
fn drop_trailing_commas(s: &str) -> String {
    let ch: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len());
    let (mut in_str, mut esc) = (false, false);
    let mut i = 0;
    while i < ch.len() {
        let c = ch[i];
        if in_str { out.push(c); if esc { esc = false; } else if c == '\\' { esc = true; } else if c == '"' { in_str = false; } i += 1; continue; }
        if c == '"' { in_str = true; out.push(c); i += 1; continue; }
        if c == ',' { let mut j = i + 1; while j < ch.len() && ch[j].is_whitespace() { j += 1; } if j < ch.len() && (ch[j] == '}' || ch[j] == ']') { i += 1; continue; } }
        out.push(c); i += 1;
    }
    out
}
// parsea JSON tolerante (como Minecraft); primero estricto (rápido), si falla limpia comentarios/comas y reintenta
fn fix_loose_numbers(s: &str) -> String {   // GSON acepta .3 (sin el 0); serde no. Convertimos .3 -> 0.3 (respeta cadenas)
    let ch: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len() + 8);
    let (mut in_str, mut esc) = (false, false);
    let mut last = ' ';
    for i in 0..ch.len() {
        let c = ch[i];
        if in_str { out.push(c); if esc { esc = false; } else if c == '\\' { esc = true; } else if c == '"' { in_str = false; } continue; }
        if c == '"' { in_str = true; out.push(c); last = '"'; continue; }
        if c == '.' {
            if i + 1 < ch.len() && ch[i + 1].is_ascii_digit() && !last.is_ascii_digit() { out.push('0'); }
            out.push('.'); last = '.'; continue;
        }
        out.push(c);
        if !c.is_whitespace() { last = c; }
    }
    out
}
fn parse_lenient(s: &str) -> Result<Value, String> {
    match serde_json::from_str::<Value>(s) {
        Ok(v) => Ok(v),
        Err(e0) => serde_json::from_str::<Value>(&fix_loose_numbers(&drop_trailing_commas(&strip_jsonc(s)))).map_err(|_| e0.to_string()),
    }
}

// El escaneo es pesado (lee/parsea todo el pack); se ejecuta en un hilo de fondo para NO congelar la ventana.
#[tauri::command]
async fn pack_doctor(path: String) -> Value {
    tauri::async_runtime::spawn_blocking(move || pack_doctor_impl(path)).await.unwrap_or_else(|_| json!({"exists": false, "error": "el análisis falló"}))
}
fn pack_doctor_impl(path: String) -> Value {
    use std::collections::{HashMap, HashSet};
    let abs = Path::new(&path);
    if path.is_empty() || !abs.exists() { return json!({"exists": false}); }

    struct F { rel: String, abs: PathBuf }
    let mut all: Vec<F> = Vec::new();
    for sub in ["data", "assets"] {
        let d = abs.join(sub); if !d.exists() { continue; }
        for e in walkdir::WalkDir::new(&d).into_iter().filter_map(|x| x.ok()) {
            if !e.file_type().is_file() { continue; }
            if let Ok(r) = e.path().strip_prefix(abs) {
                let rel = r.to_string_lossy().replace('\\', "/");
                if doctor_excluded(&rel) { continue; }
                all.push(F { rel, abs: e.path().to_path_buf() });
            }
        }
    }
    let exact: HashSet<String> = all.iter().map(|f| f.rel.clone()).collect();
    let lower_map: HashMap<String, String> = all.iter().map(|f| (f.rel.to_lowercase(), f.rel.clone())).collect();
    let has_exact = |p: &str| exact.contains(p);
    let read_json = |p: &Path| -> Result<Value, String> {
        let bytes = fs::read(p).map_err(|e| e.to_string())?;
        let s = std::str::from_utf8(&bytes).map_err(|_| "no es UTF-8".to_string())?;
        let s = s.strip_prefix('\u{feff}').unwrap_or(s);
        parse_lenient(s)   // tolera comentarios //, /* */ y comas finales, como Minecraft
    };

    let mut out: Vec<Value> = Vec::new();
    let has_data = abs.join("data").exists();
    let has_assets = abs.join("assets").exists();
    let expected_format = if has_data { 48 } else { 34 };

    // ---- manifiesto ----
    let mcmeta = abs.join("pack.mcmeta");
    if !mcmeta.exists() {
        if has_data || has_assets {
            let desc = abs.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "Pack".into());
            let content = pack_mcmeta(expected_format, &desc);
            dfind(&mut out, "pack-mcmeta-missing", "estructura", "error", "Falta pack.mcmeta",
                "El pack no tiene pack.mcmeta en la raíz; Minecraft no lo cargará.".into(), "pack.mcmeta", "auto",
                json!({"kind":"create","rel":"pack.mcmeta","content":content,"label":format!("Crear pack.mcmeta (formato {})", expected_format),"needsBackup":false,"preview":{"before":Value::Null,"after":content}}), "");
        }
    } else {
        match read_json(&mcmeta) {
            Err(e) => dfind(&mut out, "pack-mcmeta-invalid", "estructura", "error", "pack.mcmeta con JSON inválido",
                format!("No se pudo leer pack.mcmeta: {}", e), "pack.mcmeta", "manual", Value::Null, "Corrige las comas o llaves del archivo."),
            Ok(o) => match o.get("pack").and_then(|p| p.get("pack_format")).and_then(|v| v.as_i64()) {
                None => dfind(&mut out, "pack-mcmeta-invalid", "estructura", "error", "pack.mcmeta sin pack_format",
                    "Falta el campo pack.pack_format en pack.mcmeta.".into(), "pack.mcmeta", "manual", Value::Null, ""),
                // 34 (resource) y 48 (data) son los formatos válidos de 1.21.1; muchos packs combinados usan 48. Solo avisar si es claramente otro.
                Some(n) if n != 34 && n != 48 => {
                    let mut o2 = o.clone();
                    if let Some(p) = o2.get_mut("pack").and_then(|p| p.as_object_mut()) { p.insert("pack_format".into(), json!(expected_format)); }
                    let after = serde_json::to_string_pretty(&o2).unwrap_or_default() + "\n";
                    let before = fs::read_to_string(&mcmeta).unwrap_or_default();
                    dfind(&mut out, "pack-format-mismatch", "estructura", "warn", "El pack_format no es el de 1.21.1",
                        format!("pack_format = {} (para 1.21.1 se usa {} en datapack o 34 en resource pack).", n, expected_format), "pack.mcmeta", "confirmar",
                        json!({"kind":"write","rel":"pack.mcmeta","content":after,"label":format!("Cambiar pack_format a {}", expected_format),"needsBackup":true,"preview":{"before":before,"after":after}}), "");
                }
                _ => {}
            },
        }
    }

    // ---- catálogo + JSON inválido + BOM ----
    let mut species: Vec<(String, String, Value)> = Vec::new();   // (name, rel, json)
    let mut resolvers: Vec<(String, Value)> = Vec::new();
    let mut poser_names: HashSet<String> = HashSet::new();
    let mut model_names: HashSet<String> = HashSet::new();
    let mut anim_base: HashSet<String> = HashSet::new();
    let mut lang_keys: HashSet<String> = HashSet::new();
    let mut lang_files: HashMap<String, String> = HashMap::new();   // ns -> rel del en_us.json
    let mut marks: Vec<(Option<i64>, String, Option<String>, Option<String>)> = Vec::new();
    let mut used_tex: HashSet<String> = HashSet::new();
    let empty: Vec<Value> = Vec::new();
    let ns_of = |rel: &str| rel.split('/').nth(1).unwrap_or("cobblemon").to_string();

    for f in &all {
        let rl = f.rel.to_lowercase();
        let fnm = rl.rsplit('/').next().unwrap_or("");
        let is_json = rl.ends_with(".json");
        // BOM / UTF-8 (solo cabecera)
        if is_json || rl.ends_with(".geo") {
            let head = read_head(&f.abs, 512);
            if head.starts_with(&[0xEF, 0xBB, 0xBF]) {
                let prev: String = String::from_utf8_lossy(&head[3..]).chars().take(300).collect();
                dfind(&mut out, "utf8-bom", "json", "warn", "Archivo con BOM (marca de orden de bytes)",
                    "El archivo empieza con un BOM UTF-8 invisible que puede dar problemas.".into(), &f.rel, "auto",
                    json!({"kind":"strip-bom","rel":f.rel,"label":"Quitar el BOM","needsBackup":true,"preview":{"before":Value::Null,"after":prev}}), "");
            } else if head.starts_with(&[0xFF, 0xFE]) || head.starts_with(&[0xFE, 0xFF]) {
                dfind(&mut out, "utf16", "json", "error", "Archivo en UTF-16 (debería ser UTF-8)",
                    "El archivo está en UTF-16; Minecraft espera UTF-8 sin BOM.".into(), &f.rel, "manual", Value::Null, "Vuelve a guardarlo como UTF-8 sin BOM.");
            }
        }
        // JSON inválido en CUALQUIER .json (incluye .geo.json/.animation.json)
        if is_json {
            if let Err(e) = read_json(&f.abs) {
                let cat = classify(&f.rel).unwrap_or_else(|| "otros".into());
                if e.contains("UTF-8") {
                    dfind(&mut out, "encoding", "json", "error", "Archivo con codificación incorrecta",
                        format!("Este archivo ({}) no está en UTF-8; las tildes/ñ se corrompen y Minecraft no lo lee bien.", cat), &f.rel, "manual", Value::Null, "Vuelve a guardarlo como UTF-8 (sin BOM).");
                } else {
                    dfind(&mut out, "json-invalid", "json", "error", "Archivo JSON con errores",
                        format!("No se pudo leer este archivo ({}): {}", cat, e), &f.rel, "manual", Value::Null, "Suele ser una coma de más, una llave sin cerrar o comillas.");
                }
            }
        }
        // catálogo
        if rl.contains("/posers/") && is_json { poser_names.insert(fnm.strip_suffix(".json").unwrap_or(fnm).to_string()); }
        if rl.contains("/models/") && (rl.ends_with(".geo.json") || rl.ends_with(".geo")) {
            let b = fnm.strip_suffix(".geo.json").or_else(|| fnm.strip_suffix(".geo")).unwrap_or(fnm);
            model_names.insert(b.to_string());
        }
        if rl.contains("/animations/") && rl.ends_with(".animation.json") { anim_base.insert(fnm.strip_suffix(".animation.json").unwrap_or(fnm).to_string()); }
        if rl.contains("/lang/") && is_json {
            if let Ok(o) = read_json(&f.abs) { if let Some(m) = o.as_object() { for k in m.keys() { lang_keys.insert(k.to_lowercase()); } } }
            if fnm == "en_us.json" { lang_files.entry(ns_of(&f.rel)).or_insert_with(|| f.rel.clone()); }
        }
        if f.rel.starts_with("data/") && rl.contains("/species/") && is_json
            && !rl.contains("species_additions") && !rl.contains("species_features") && !rl.contains("species_feature_assignments") {
            if let Ok(o) = read_json(&f.abs) {
                let name = o["name"].as_str().map(|s| s.to_string()).unwrap_or_else(|| fnm.strip_suffix(".json").unwrap_or(fnm).to_string()).to_lowercase();
                species.push((name, f.rel.clone(), o));
            }
        }
        let is_resolver = is_json && (rl.contains("/resolvers/") || (f.rel.starts_with("assets/") && rl.contains("/species/")));
        if is_resolver { if let Ok(o) = read_json(&f.abs) { resolvers.push((f.rel.clone(), o)); } }
        if rl.contains("/marks/") && is_json {
            if let Ok(o) = read_json(&f.abs) { marks.push((o["indexNumber"].as_i64(), f.rel.clone(), o["texture"].as_str().map(|s| s.to_string()), o["name"].as_str().map(|s| s.to_string()))); }
        }
    }

    // ---- resolvers: campos, texturas/modelos/posers, case-mismatch ----
    let mut resolver_sp: HashSet<String> = HashSet::new();
    for (rel, o) in &resolvers {
        match o["species"].as_str() {
            Some(sp) if !sp.is_empty() => { resolver_sp.insert(sp.rsplit(':').next().unwrap_or(sp).to_lowercase()); }
            _ => dfind(&mut out, "resolver-no-species", "referencias", "error", "Apariencia (resolver) sin campo 'species'",
                "Este resolver no indica a qué Pokémon pertenece (falta 'species').".into(), rel, "manual", Value::Null, "Añade \"species\": \"cobblemon:<nombre>\"."),
        }
        for v in o["variations"].as_array().unwrap_or(&empty) {
            if let Some(t) = v["texture"].as_str() {
                if let Some(p) = vfy_asset_path(t) { used_tex.insert(p.to_lowercase()); }
                if vfy_own(t) { if let Some(p) = vfy_asset_path(t) {
                    if !has_exact(&p) {
                        if let Some(real) = lower_map.get(&p.to_lowercase()) {
                            // existe con otro casing -> rompe en Linux
                            let new_ref = real.strip_prefix("assets/").map(|x| x.replacen('/', ":", 1)).unwrap_or_else(|| t.to_string());
                            dfind(&mut out, "texture-case", "referencias", "error", "Textura con mayúsculas/minúsculas que no coinciden",
                                format!("El resolver pide «{}» pero el archivo real es «{}». Funciona en Windows pero ROMPE en servidores Linux.", t, new_ref), rel, "auto",
                                json!({"kind":"replace-text","rel":rel,"find":format!("\"{}\"", t),"replace":format!("\"{}\"", new_ref),"label":"Corregir mayúsculas de la ruta","needsBackup":true,"preview":{"before":t,"after":new_ref}}), "");
                        } else {
                            dfind(&mut out, "resolver-texture-missing", "referencias", "error", "Apariencia con textura que no existe",
                                format!("Falta la textura: {} → el Pokémon se vería sin textura.", t), rel, "manual", Value::Null, "Aporta el .png en esa ruta dentro del pack.");
                        }
                    }
                } }
            }
            for ly in v["layers"].as_array().unwrap_or(&empty) {
                if let Some(t) = ly["texture"].as_str() { if let Some(p) = vfy_asset_path(t) { used_tex.insert(p.to_lowercase());
                    if vfy_own(t) && !has_exact(&p) && !lower_map.contains_key(&p.to_lowercase()) {
                        dfind(&mut out, "resolver-layer-missing", "referencias", "warn", "Capa de textura que falta",
                            format!("Falta una capa de textura: {}", t), rel, "manual", Value::Null, "");
                    }
                } }
            }
            if let Some(ps) = v["poser"].as_str() { if vfy_own(ps) {
                let nm = ps.rsplit(|c| c == '/' || c == ':').next().unwrap_or(ps).to_lowercase();
                if !poser_names.contains(&nm) {
                    dfind(&mut out, "resolver-poser-missing", "referencias", "warn", "Poser que falta",
                        format!("El poser \"{}\" no se encuentra (faltarían animaciones).", ps), rel, "manual", Value::Null, "Crea/exporta el poser o corrige el id.");
                }
            } }
            if let Some(md) = v["model"].as_str() { if vfy_own(md) {
                let seg = md.rsplit(|c| c == '/' || c == ':').next().unwrap_or(md);
                let nm = if seg.to_lowercase().ends_with(".geo") { &seg[..seg.len() - 4] } else { seg };
                if !model_names.contains(&nm.to_lowercase()) {
                    dfind(&mut out, "resolver-model-missing", "referencias", "warn", "Modelo .geo que falta",
                        format!("El modelo \"{}\" no se encuentra (¿exportaste el .geo de Blockbench?).", md), rel, "manual", Value::Null, "Exporta el modelo desde Blockbench.");
                }
            } }
        }
    }

    // ---- species: campos, tipos, nombre, duplicados, dex, ids ----
    let mut name_files: HashMap<String, Vec<String>> = HashMap::new();
    let mut dex_nums: HashMap<i64, Vec<String>> = HashMap::new();
    let species_set: HashSet<String> = species.iter().map(|(n, _, _)| n.clone()).collect();
    for (name, rel, o) in &species {
        name_files.entry(name.clone()).or_default().push(rel.clone());
        if let Some(n) = o["nationalPokedexNumber"].as_i64() { dex_nums.entry(n).or_default().push(rel.clone()); }
        // campos obligatorios
        let mut missing: Vec<&str> = Vec::new();
        if o["name"].as_str().map(|s| s.is_empty()).unwrap_or(true) { missing.push("name"); }
        if o["primaryType"].as_str().map(|s| s.is_empty()).unwrap_or(true) { missing.push("primaryType"); }
        if !o["baseStats"].is_object() { missing.push("baseStats"); }
        if !missing.is_empty() {
            dfind(&mut out, "species-missing-fields", "datapack", "error", "A un Pokémon le faltan campos obligatorios",
                format!("Faltan: {}.", missing.join(", ")), rel, "manual", Value::Null, "Define al menos name, primaryType y baseStats.");
        }
        // tipos
        for key in ["primaryType", "secondaryType"] {
            if let Some(t) = o[key].as_str() { if !t.is_empty() {
                let tl = t.trim().to_lowercase();
                if !POKE_TYPES.contains(&tl.as_str()) {
                    dfind(&mut out, "species-bad-type", "datapack", if key == "primaryType" { "error" } else { "warn" }, "Tipo elemental inválido",
                        format!("«{}» no es un tipo válido en {}.", t, key), rel, "manual", Value::Null, "Tipos: normal, fire, water, grass, electric, ice, fighting, poison, ground, flying, psychic, bug, rock, ghost, dragon, dark, steel, fairy.");
                } else if t != tl {
                    // mismo tipo pero mal escrito (mayúsculas/espacios) -> sustitución segura
                    dfind(&mut out, "species-type-case", "datapack", "warn", "Tipo con mayúsculas/espacios",
                        format!("«{}» debería escribirse «{}».", t, tl), rel, "confirmar",
                        json!({"kind":"replace-text","rel":rel,"find":format!("\"{}\"", t),"replace":format!("\"{}\"", tl),"label":format!("Corregir tipo a «{}»", tl),"needsBackup":true,"preview":{"before":t,"after":tl}}), "");
                }
            } }
        }
        // id inválido
        if !valid_resource_id(name) {
            dfind(&mut out, "species-bad-id", "nombres", "error", "Nombre de Pokémon con caracteres inválidos",
                format!("«{}» tiene mayúsculas, espacios o símbolos. Los ids de Minecraft deben ser minúsculas a-z, 0-9, _-.", name), rel, "manual", Value::Null, &slugify(name));
        }
        // filename vs name
        let fname = rel.rsplit('/').next().unwrap_or("").strip_suffix(".json").unwrap_or("").to_lowercase();
        let real_name = o["name"].as_str().unwrap_or("");
        if real_name.is_empty() && !fname.is_empty() {
            dfind(&mut out, "species-name-from-file", "nombres", "warn", "El Pokémon no tiene 'name'",
                format!("Le falta el campo name; se usaría el del archivo: «{}».", fname), rel, "confirmar",
                json!({"kind":"merge-json","rel":rel,"key":"name","value":fname,"label":format!("Poner name = «{}»", fname),"needsBackup":true,"preview":{"before":Value::Null,"after":format!("name: \"{}\"", fname)}}), "");
        } else if !real_name.is_empty() && real_name.to_lowercase() != fname && !fname.is_empty() {
            dfind(&mut out, "species-filename-mismatch", "nombres", "warn", "El nombre del archivo no coincide con el id del Pokémon",
                format!("Archivo «{}» pero name «{}». Los resolvers/spawns ligan por name.", fname, real_name), rel, "manual", Value::Null, "");
        }
        // sin resolver
        if !resolver_sp.iter().any(|r| r == name) {
            dfind(&mut out, "species-no-resolver", "referencias", "warn", "Pokémon sin apariencia (resolver)",
                format!("«{}» no tiene resolver: saldrá como sustituto/invisible.", name), rel, "manual", Value::Null, "Crea su archivo de apariencia (resolver).");
        }
        // sin lang name
        if !lang_keys.contains(&format!("cobblemon.species.{}.name", name)) {
            let ns = ns_of(rel);
            let lang_rel = lang_files.get(&ns).cloned().unwrap_or_else(|| format!("assets/{}/lang/en_us.json", ns));
            let key = format!("cobblemon.species.{}.name", name);
            let val = title_case(name);
            let prev = format!("{} = \"{}\"", key, val);
            dfind(&mut out, "species-no-lang", "lang", "warn", "Pokémon sin nombre en el juego (falta texto)",
                format!("«{}» no tiene nombre traducido (lang).", name), rel, "confirmar",
                json!({"kind":"merge-json","rel":lang_rel,"key":key,"value":val,"label":format!("Añadir nombre «{}» al lang", title_case(name)),"needsBackup":true,"preview":{"before":Value::Null,"after":prev}}), "");
        }
        // evoluciones
        for ev in o["evolutions"].as_array().unwrap_or(&empty) {
            if let Some(res) = ev["result"].as_str() {
                let base = res.split(|c| c == ' ' || c == ':').filter(|s| !s.is_empty()).next().unwrap_or("");
                let token = base.rsplit(':').next().unwrap_or(base).to_lowercase();
                let own = res.contains(':') && vfy_own(res);
                if !token.is_empty() && (own || !res.contains(':')) && !species_set.contains(&token) {
                    dfind(&mut out, "evo-result-missing", "referencias", "warn", "Una evolución lleva a un Pokémon que no existe en el pack",
                        format!("La evolución apunta a «{}», que no está definido aquí.", res), rel, "manual", Value::Null, "Si es de Cobblemon base, usa el prefijo cobblemon:.");
                }
            }
        }
    }
    for (name, files) in &name_files { if files.len() > 1 {
        dfind(&mut out, "species-dup-name", "nombres", "error", "Dos Pokémon con el mismo nombre",
            format!("«{}» está definido en {} archivos; uno pisará al otro.", name, files.len()), &files[0], "manual", Value::Null, &files.join(" · "));
    } }
    for (num, files) in &dex_nums { if files.len() > 1 {
        dfind(&mut out, "species-dup-dex", "datapack", "warn", "Dos Pokémon comparten número de Pokédex",
            format!("El nº {} lo usan {} Pokémon.", num, files.len()), &files[0], "manual", Value::Null, &files.join(" · "));
    } }

    // resolver -> species inexistente (huérfano), solo ns propio
    for (rel, o) in &resolvers {
        if let Some(sp) = o["species"].as_str() {
            if vfy_own(sp) {
                let token = sp.rsplit(':').next().unwrap_or(sp).to_lowercase();
                if !species_set.contains(&token) {
                    dfind(&mut out, "resolver-orphan", "referencias", "warn", "Apariencia que apunta a un Pokémon inexistente",
                        format!("El resolver es de «{}», que no existe en el pack.", sp), rel, "manual", Value::Null, "");
                }
            }
        }
    }

    // ---- spawns, dex_entries, species_additions (referencias cruzadas) ----
    for f in &all {
        let cat = match classify(&f.rel) { Some(c) => c, None => continue };
        let o = match read_json(&f.abs) { Ok(o) => o, Err(_) => continue };
        match cat.as_str() {
            "spawns" => for (i, sp) in o["spawns"].as_array().unwrap_or(&empty).iter().enumerate() {
                match sp["pokemon"].as_str() {
                    None => dfind(&mut out, "spawn-no-pokemon", "datapack", "error", "Spawn sin 'pokemon'",
                        format!("La entrada de spawn #{} no indica qué Pokémon aparece.", i + 1), &f.rel, "manual", Value::Null, ""),
                    Some(p) => { let token = p.split(' ').next().unwrap_or(p); let base = token.rsplit(':').next().unwrap_or(token).to_lowercase();
                        if token.contains(':') && vfy_own(token) && !species_set.contains(&base) {
                            dfind(&mut out, "spawn-missing-species", "referencias", "warn", "Un spawn usa una especie inexistente",
                                format!("El spawn hace aparecer «{}», que no está definido en el pack.", p), &f.rel, "manual", Value::Null, "");
                        } }
                }
            },
            "dexentries" => if let Some(sid) = o["speciesId"].as_str() {
                if vfy_own(sid) { let base = sid.rsplit(':').next().unwrap_or(sid).to_lowercase();
                    if !species_set.contains(&base) {
                        dfind(&mut out, "dexentry-missing", "referencias", "error", "Entrada de Pokédex de un Pokémon inexistente",
                            format!("La entrada apunta a «{}», que no existe en el pack.", sid), &f.rel, "manual", Value::Null, "");
                    }
                }
            },
            "additions" => if let Some(t) = o["target"].as_str() {
                if vfy_own(t) { let base = t.rsplit(':').next().unwrap_or(t).to_lowercase();
                    if !species_set.contains(&base) {
                        dfind(&mut out, "addition-target-missing", "referencias", "error", "Edición de Pokémon que apunta a una especie inexistente",
                            format!("Modifica «{}», que no existe en el pack.", t), &f.rel, "manual", Value::Null, "Si editas uno de Cobblemon base, usa cobblemon:.");
                    }
                } else if t.is_empty() || !t.contains(':') {
                    dfind(&mut out, "addition-target-bad", "referencias", "error", "Edición de Pokémon con 'target' mal formado",
                        "El campo target debe ser \"ns:nombre\".".into(), &f.rel, "manual", Value::Null, "");
                }
            },
            _ => {}
        }
    }

    // ---- marcas ----
    let mut seen_idx: HashMap<i64, bool> = HashMap::new();
    let mut max_idx = -1i64;
    for (idx, _f, _t, _n) in &marks { if let Some(i) = idx { if *i > max_idx { max_idx = *i; } } }
    for (idx, file, texture, mname) in &marks {
        if let Some(i) = idx { if seen_idx.contains_key(i) {
            dfind(&mut out, "mark-dup-index", "datapack", "error", "Dos marcas con el mismo número",
                format!("El número {} está repetido (colisionan).", i), file, "manual", Value::Null, "Asigna números distintos.");
        } else { seen_idx.insert(*i, true); } }
        if let Some(t) = texture { if vfy_own(t) { if let Some(p) = vfy_asset_path(t) { if !has_exact(&p) && !lower_map.contains_key(&p.to_lowercase()) {
            dfind(&mut out, "mark-no-texture", "referencias", "warn", "Marca sin su textura", format!("Falta la textura: {}", t), file, "manual", Value::Null, "");
        } } } }
        if let Some(n) = mname { if !lang_keys.contains(&format!("{}.title", n).to_lowercase()) {
            let ns = ns_of(file);
            let lang_rel = lang_files.get(&ns).cloned().unwrap_or_else(|| format!("assets/{}/lang/en_us.json", ns));
            let key = format!("{}.title", n);
            let val = title_case(n);
            let prev = format!("{} = \"{}\"", key, val);
            dfind(&mut out, "mark-no-lang", "lang", "warn", "Marca sin nombre en el juego",
                format!("La marca «{}» no tiene nombre traducido.", n), file, "confirmar",
                json!({"kind":"merge-json","rel":lang_rel,"key":key,"value":val,"label":"Añadir el nombre de la marca al lang","needsBackup":true,"preview":{"before":Value::Null,"after":prev}}), "");
        } }
    }

    // ---- texturas huérfanas (info) ----
    for f in &all {
        let rl = f.rel.to_lowercase();
        if !(rl.contains("/textures/pokemon/") && rl.ends_with(".png")) { continue; }
        if rl.ends_with("_shiny.png") || rl.ends_with("_emissive.png") { continue; }
        if !used_tex.contains(&rl) {
            dfind(&mut out, "orphan-texture", "huerfanos", "info", "Textura que no usa ninguna apariencia",
                "Esta textura no la referencia ningún resolver (puede sobrar o usarse por convención).".into(), &f.rel, "manual", Value::Null, "");
        }
    }

    // ---- resumen de salud ----
    let count = |lvl: &str| out.iter().filter(|x| x["level"] == json!(lvl)).count();
    let (err, warn, info) = (count("error"), count("warn"), count("info"));
    let fa = out.iter().filter(|x| x["safety"] == json!("auto") && x.get("fix").is_some()).count();
    let fc = out.iter().filter(|x| x["safety"] == json!("confirmar") && x.get("fix").is_some()).count();
    let manual = out.len() - fa - fc;
    let score = (100i64 - (err as i64) * 12 - (warn as i64) * 3 - (info as i64)).max(0);
    let label = if err > 0 { "con errores" } else if warn > 0 { "con avisos" } else { "saludable" };
    json!({"exists": true, "total": all.len(), "findings": out,
        "health": {"score": score, "label": label, "error": err, "warn": warn, "info": info, "fixableAuto": fa, "fixableConfirm": fc, "manual": manual}})
}

#[derive(Deserialize)]
struct DoctorFix { #[serde(default)] id: String, kind: String, #[serde(default)] rel: String, #[serde(default)] from: String, #[serde(default)] to: String,
    #[serde(default)] content: String, #[serde(default)] key: String, #[serde(default)] value: String, #[serde(default)] find: String, #[serde(default)] replace: String }

fn doctor_safe_path(root: &Path, rel: &str) -> Option<PathBuf> {
    if rel.is_empty() || rel.contains("..") { return None; }
    let full = root.join(rel.replace('\\', "/"));
    if full.starts_with(root) { Some(full) } else { None }
}
fn doctor_ts() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    format!("{}", SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0))
}
// copia el original a _backup_doctor/<ts>/<rel> antes de tocarlo; devuelve si respaldó (false = el archivo no existía)
fn doctor_backup(abs: &Path, bdir: &Path, rel: &str) -> Result<bool, String> {
    let src = doctor_safe_path(abs, rel).ok_or_else(|| "ruta".to_string())?;
    if !src.is_file() { return Ok(false); }
    let data = fs::read(&src).map_err(|e| e.to_string())?;
    write_atomic(&bdir.join(rel.replace('\\', "/")), &data).map_err(|e| e.to_string())?;
    Ok(true)
}
// aplica UN arreglo como transformación sobre el estado ACTUAL del disco (a prueba de colisiones en lote) y devuelve su entrada de manifest
fn doctor_apply_one(abs: &Path, bdir: &Path, fx: &DoctorFix) -> Result<Value, String> {
    match fx.kind.as_str() {
        "create" | "write" => {
            let full = doctor_safe_path(abs, &fx.rel).ok_or_else(|| "ruta".to_string())?;
            let backed = doctor_backup(abs, bdir, &fx.rel)?;   // respalda aunque sea "create" si el destino ya existía
            write_atomic(&full, fx.content.as_bytes()).map_err(|e| e.to_string())?;
            Ok(json!({"id": fx.id, "rel": fx.rel, "backed": backed}))
        }
        "strip-bom" => {
            let full = doctor_safe_path(abs, &fx.rel).ok_or_else(|| "ruta".to_string())?;
            let backed = doctor_backup(abs, bdir, &fx.rel)?;
            let bytes = fs::read(&full).map_err(|e| e.to_string())?;
            let body = if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) { &bytes[3..] } else { &bytes[..] };
            write_atomic(&full, body).map_err(|e| e.to_string())?;
            Ok(json!({"id": fx.id, "rel": fx.rel, "backed": backed}))
        }
        "replace-text" => {
            let full = doctor_safe_path(abs, &fx.rel).ok_or_else(|| "ruta".to_string())?;
            let backed = doctor_backup(abs, bdir, &fx.rel)?;
            let s = fs::read_to_string(&full).map_err(|e| e.to_string())?;
            let n = s.replacen(&fx.find, &fx.replace, 1);   // re-leído del disco actual: si otro fix ya tocó el fichero, se aplica encima
            write_atomic(&full, n.as_bytes()).map_err(|e| e.to_string())?;
            Ok(json!({"id": fx.id, "rel": fx.rel, "backed": backed}))
        }
        "merge-json" => {
            let full = doctor_safe_path(abs, &fx.rel).ok_or_else(|| "ruta".to_string())?;
            let backed = doctor_backup(abs, bdir, &fx.rel)?;
            let mut o = fs::read_to_string(&full).ok()
                .and_then(|s| parse_lenient(s.strip_prefix('\u{feff}').unwrap_or(&s)).ok())
                .unwrap_or_else(|| json!({}));   // lee el estado ACTUAL tolerando comentarios (acumula claves entre fixes del mismo lang)
            if !o.is_object() { o = json!({}); }
            if let Some(m) = o.as_object_mut() { m.insert(fx.key.clone(), json!(fx.value)); }
            let txt = serde_json::to_string_pretty(&o).map_err(|e| e.to_string())? + "\n";
            write_atomic(&full, txt.as_bytes()).map_err(|e| e.to_string())?;
            Ok(json!({"id": fx.id, "rel": fx.rel, "backed": backed}))
        }
        "delete" => {
            let full = doctor_safe_path(abs, &fx.rel).ok_or_else(|| "ruta".to_string())?;
            let backed = doctor_backup(abs, bdir, &fx.rel)?;
            if full.exists() { fs::remove_file(&full).map_err(|e| e.to_string())?; }
            Ok(json!({"id": fx.id, "rel": fx.rel, "backed": backed}))
        }
        "move" => {
            let src = doctor_safe_path(abs, &fx.from).ok_or_else(|| "ruta".to_string())?;
            let dst = doctor_safe_path(abs, &fx.to).ok_or_else(|| "ruta".to_string())?;
            let backed = doctor_backup(abs, bdir, &fx.from)?;
            let data = fs::read(&src).map_err(|e| e.to_string())?;
            write_atomic(&dst, &data).map_err(|e| e.to_string())?;
            fs::remove_file(&src).map_err(|e| e.to_string())?;
            Ok(json!({"id": fx.id, "action": "move", "from": fx.from, "to": fx.to, "backed": backed}))
        }
        other => Err(format!("acción desconocida: {}", other)),
    }
}

#[tauri::command]
async fn doctor_apply_fixes(path: String, fixes: Vec<DoctorFix>) -> Value {
    tauri::async_runtime::spawn_blocking(move || doctor_apply_fixes_impl(path, fixes)).await.unwrap_or_else(|_| json!({"ok": false, "error": "el proceso de arreglo falló"}))
}
fn doctor_apply_fixes_impl(path: String, fixes: Vec<DoctorFix>) -> Value {
    let abs = Path::new(&path);
    if path.is_empty() || !abs.exists() { return json!({"ok": false, "error": "pack no encontrado"}); }
    // validar TODO antes de tocar nada
    for fx in &fixes {
        match fx.kind.as_str() {
            "create" | "write" | "merge-json" | "replace-text" | "strip-bom" | "delete" =>
                if doctor_safe_path(abs, &fx.rel).is_none() { return json!({"ok": false, "error": format!("ruta inválida: {}", fx.rel)}); },
            "move" => {
                if doctor_safe_path(abs, &fx.from).is_none() || doctor_safe_path(abs, &fx.to).is_none() { return json!({"ok": false, "error": "ruta inválida"}); }
                if doctor_safe_path(abs, &fx.to).map(|p| p.exists()).unwrap_or(true) { return json!({"ok": false, "error": format!("el destino ya existe: {}", fx.to)}); }
            }
            other => return json!({"ok": false, "error": format!("acción desconocida: {}", other)}),
        }
    }
    let ts = doctor_ts();
    let bdir = abs.join("_backup_doctor").join(&ts);
    let mut manifest: Vec<Value> = Vec::new();
    let mut failed: Vec<Value> = Vec::new();
    let mut applied = 0;
    for fx in &fixes {
        match doctor_apply_one(abs, &bdir, fx) {
            Ok(entry) => { manifest.push(entry); applied += 1; }
            Err(e) => failed.push(json!({"id": fx.id, "error": e})),
        }
    }
    let _ = write_atomic(&bdir.join("manifest.json"), serde_json::to_string_pretty(&json!({"timestamp": ts, "fixes": manifest})).unwrap_or_default().as_bytes());
    json!({"ok": failed.is_empty(), "timestamp": ts, "applied": applied, "failed": failed, "backupDir": format!("_backup_doctor/{}", ts)})
}

#[tauri::command]
fn doctor_list_backups(path: String) -> Value {
    let abs = Path::new(&path);
    let dir = abs.join("_backup_doctor");
    let mut list: Vec<Value> = Vec::new();
    if dir.exists() { for e in fs::read_dir(&dir).into_iter().flatten().filter_map(|x| x.ok()) {
        if !e.path().is_dir() { continue; }
        let man = e.path().join("manifest.json");
        let (ts, count) = match fs::read_to_string(&man).ok().and_then(|s| serde_json::from_str::<Value>(&s).ok()) {
            Some(m) => (m["timestamp"].as_str().unwrap_or("").to_string(), m["fixes"].as_array().map(|a| a.len()).unwrap_or(0)),
            None => (e.file_name().to_string_lossy().to_string(), 0),
        };
        list.push(json!({"timestamp": ts, "count": count, "dir": e.file_name().to_string_lossy()}));
    } }
    list.sort_by(|a, b| b["timestamp"].as_str().unwrap_or("").cmp(a["timestamp"].as_str().unwrap_or("")));
    json!({"backups": list})
}

#[tauri::command]
async fn doctor_restore_backup(path: String, timestamp: String) -> Value {
    tauri::async_runtime::spawn_blocking(move || doctor_restore_backup_impl(path, timestamp)).await.unwrap_or_else(|_| json!({"ok": false, "error": "la restauración falló"}))
}
fn doctor_restore_backup_impl(path: String, timestamp: String) -> Value {
    let abs = Path::new(&path);
    if timestamp.contains("..") || timestamp.contains('/') || timestamp.contains('\\') { return json!({"ok": false, "error": "id inválido"}); }
    let bdir = abs.join("_backup_doctor").join(&timestamp);
    let man = match fs::read_to_string(bdir.join("manifest.json")).ok().and_then(|s| serde_json::from_str::<Value>(&s).ok()) {
        Some(m) => m, None => return json!({"ok": false, "error": "copia no encontrada"}),
    };
    let mut restored = 0;
    for fx in man["fixes"].as_array().cloned().unwrap_or_default() {
        let ok: Option<()> = (|| {
            if fx["action"].as_str() == Some("move") {
                let from = doctor_safe_path(abs, fx["from"].as_str()?)?;
                let to = doctor_safe_path(abs, fx["to"].as_str()?)?;
                if to.exists() { let d = fs::read(&to).ok()?; write_atomic(&from, &d).ok()?; fs::remove_file(&to).ok()?; }
            } else {
                let rel = fx["rel"].as_str()?;
                let dst = doctor_safe_path(abs, rel)?;
                if fx["backed"].as_bool().unwrap_or(false) {
                    let backup = bdir.join(rel.replace('\\', "/"));   // había original: lo reponemos
                    if backup.is_file() { let d = fs::read(&backup).ok()?; write_atomic(&dst, &d).ok()?; }
                } else if dst.exists() {
                    fs::remove_file(&dst).ok()?;   // lo habíamos creado de cero: lo quitamos
                }
            }
            Some(())
        })();
        if ok.is_some() { restored += 1; }
    }
    json!({"ok": true, "restored": restored})
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

// ===== Exportar el pack ABIERTO a .zip (tal cual, conservando su pack.mcmeta) =====
fn collect_pack_files(root: &Path, sub: Option<&str>) -> Vec<(String, PathBuf)> {
    let base = match sub { Some(s) => root.join(s), None => root.to_path_buf() };
    if !base.exists() { return Vec::new(); }
    let mut v = Vec::new();
    for e in walkdir::WalkDir::new(&base).into_iter().filter_map(|x| x.ok()) {
        if !e.file_type().is_file() { continue; }
        if let Ok(r) = e.path().strip_prefix(root) {
            let rel = r.to_string_lossy().replace('\\', "/");
            if rel.starts_with("_backup_doctor/") || rel.contains("/_backup_doctor/") || rel.ends_with(".cs_tmp") { continue; }   // no incluir backups del Doctor ni temporales
            v.push((rel, e.path().to_path_buf()));
        }
    }
    v
}
// escribe los archivos en un .zip; si no hay pack.mcmeta entre ellos, usa el de respaldo (real o generado)
fn zip_to(zip_path: &Path, files: &[(String, PathBuf)], mcmeta_fallback: Option<Vec<u8>>) -> Result<(u32, u64), String> {
    let file = fs::File::create(zip_path).map_err(|e| e.to_string())?;
    let mut zw = zip::ZipWriter::new(file);
    let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    let mut count = 0u32;
    let mut has_meta = false;
    for (rel, abs) in files {
        if rel == "pack.mcmeta" { has_meta = true; }
        zw.start_file(rel.clone(), opts).map_err(|e| e.to_string())?;
        zw.write_all(&fs::read(abs).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
        count += 1;
    }
    if !has_meta { if let Some(b) = mcmeta_fallback {
        zw.start_file("pack.mcmeta", opts).map_err(|e| e.to_string())?;
        zw.write_all(&b).map_err(|e| e.to_string())?; count += 1;
    } }
    zw.finish().map_err(|e| e.to_string())?;
    Ok((count, fs::metadata(zip_path).map(|m| m.len()).unwrap_or(0)))
}

#[tauri::command]
async fn export_pack_combined(app: tauri::AppHandle, path: String) -> Result<Value, String> {
    let root = PathBuf::from(&path);
    if path.is_empty() || !root.exists() { return Err("No hay ningún pack abierto.".into()); }
    let name = root.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "pack".into());
    let dest = app.dialog().file().set_file_name(format!("{}.zip", name)).add_filter("ZIP", &["zip"]).blocking_save_file();
    let dest = match dest { Some(d) => d.to_string(), None => return Ok(json!({"cancelled": true})) };
    let has_data = root.join("data").exists();
    tauri::async_runtime::spawn_blocking(move || -> Result<Value, String> {
        let files = collect_pack_files(&root, None);
        if files.is_empty() { return Err("El pack está vacío.".into()); }
        let fallback = pack_mcmeta(if has_data { 48 } else { 34 }, &name).into_bytes();   // solo si el pack no tuviera pack.mcmeta
        let (count, size) = zip_to(Path::new(&dest), &files, Some(fallback))?;
        Ok(json!({"ok": true, "file": dest, "files": count, "sizeKB": size / 1024}))
    }).await.map_err(|e| e.to_string())?
}

#[tauri::command]
async fn export_pack_split(app: tauri::AppHandle, path: String) -> Result<Value, String> {
    let root = PathBuf::from(&path);
    if path.is_empty() || !root.exists() { return Err("No hay ningún pack abierto.".into()); }
    let name = root.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "pack".into());
    let dest_dir = app.dialog().file().blocking_pick_folder();
    let dest_dir = match dest_dir { Some(d) => PathBuf::from(d.to_string()), None => return Ok(json!({"cancelled": true})) };
    tauri::async_runtime::spawn_blocking(move || -> Result<Value, String> {
        let real_meta = fs::read(root.join("pack.mcmeta")).ok();   // conservar el pack.mcmeta REAL si existe
        let mut zips = Vec::new();
        for (sub, suffix, fmt, kind) in [("data", "_datapack.zip", 48i64, "datapack"), ("assets", "_resourcepack.zip", 34i64, "resourcepack")] {
            if !root.join(sub).exists() { continue; }
            let files = collect_pack_files(&root, Some(sub));
            let meta = real_meta.clone().unwrap_or_else(|| pack_mcmeta(fmt, &name).into_bytes());
            let zip_path = dest_dir.join(format!("{}{}", name, suffix));
            let (count, size) = zip_to(&zip_path, &files, Some(meta))?;
            zips.push(json!({"kind": kind, "file": zip_path.to_string_lossy(), "files": count, "sizeKB": size / 1024}));
        }
        if zips.is_empty() { return Err("El pack no tiene carpeta data/ ni assets/.".into()); }
        Ok(json!({"ok": true, "zips": zips}))
    }).await.map_err(|e| e.to_string())?
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

// "Guardar como…": abre un diálogo nativo y escribe los bytes (base64) en la ruta elegida. Devuelve la ruta o None si se cancela.
#[tauri::command]
fn save_export(app: tauri::AppHandle, name: String, b64: String) -> Result<Option<String>, String> {
    let ext = Path::new(&name).extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
    let mut dlg = app.dialog().file().set_file_name(name.clone());
    if !ext.is_empty() { dlg = dlg.add_filter(ext.to_uppercase(), &[ext.as_str()]); }
    match dlg.blocking_save_file() {
        Some(fp) => {
            let path = fp.to_string();
            let raw = b64.rsplit(',').next().unwrap_or(&b64).trim();
            let bytes = base64::engine::general_purpose::STANDARD.decode(raw).map_err(|e| e.to_string())?;
            if bytes.is_empty() { return Err("no hay datos que guardar".into()); }
            fs::write(&path, &bytes).map_err(|e| e.to_string())?;
            Ok(Some(path))
        }
        None => Ok(None),
    }
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
            check_and_update, restart_app, set_unsaved, save_export,
            pack_doctor, doctor_apply_fixes, doctor_list_backups, doctor_restore_backup,
            export_pack_combined, export_pack_split
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
