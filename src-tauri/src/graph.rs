use crate::models::{GraphColorGroup, GraphData, GraphEdge, GraphNode, NoteContent};
use regex::Regex;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

fn wikilink_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\[\[([^\]|#]+)(?:#[^\]|]+)?(?:\|[^\]]+)?\]\]").unwrap())
}

fn rgb_int_to_hex(rgb: u64) -> String {
    format!("#{:06x}", rgb & 0xFFFFFF)
}

fn collect_md_files(dir: &Path, base: &Path, out: &mut Vec<(String, PathBuf)>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') {
            continue;
        }
        if path.is_dir() {
            collect_md_files(&path, base, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("md") {
            if let Ok(rel) = path.strip_prefix(base) {
                out.push((rel.to_string_lossy().replace('\\', "/"), path));
            }
        }
    }
}

fn classify_node(rel_path: &str, stem: &str, custom_colors: &HashMap<String, String>) -> (String, String) {
    if rel_path.starts_with("dersler/") {
        return (
            "Dersler".to_string(),
            custom_colors
                .get("dersler")
                .cloned()
                .unwrap_or_else(|| "#3b7ee6".to_string()),
        );
    }
    if rel_path.starts_with("hatalar/") {
        return (
            "Hatalar".to_string(),
            custom_colors
                .get("hatalar")
                .cloned()
                .unwrap_or_else(|| "#ef4444".to_string()),
        );
    }
    if rel_path.starts_with("projeler/") {
        return (
            "Projeler".to_string(),
            custom_colors
                .get("projeler")
                .cloned()
                .unwrap_or_else(|| "#22c55e".to_string()),
        );
    }
    let owner_files = [
        "sahip",
        "kararlar",
        "tasarim-dili",
        "arastirma-tercihleri",
        "acik_konular",
        "projeler",
    ];
    if owner_files.contains(&stem) {
        return (
            "Sahip & Kurallar".to_string(),
            custom_colors
                .get("sahip")
                .cloned()
                .unwrap_or_else(|| "#f59e0b".to_string()),
        );
    }
    ("Genel".to_string(), "#94a3b8".to_string())
}

fn read_obsidian_graph_colors(hafiza_dir: &Path) -> (HashMap<String, String>, bool) {
    let mut colors = HashMap::new();
    let mut show_arrow = true;
    let graph_json_path = hafiza_dir.join(".obsidian").join("graph.json");
    if let Ok(raw) = fs::read_to_string(graph_json_path) {
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(&raw) {
            if let Some(arr) = val.get("showArrow").and_then(|v| v.as_bool()) {
                show_arrow = arr;
            }
            if let Some(groups) = val.get("colorGroups").and_then(|v| v.as_array()) {
                for g in groups {
                    let query = g.get("query").and_then(|q| q.as_str()).unwrap_or("");
                    let rgb = g
                        .get("color")
                        .and_then(|c| c.get("rgb"))
                        .and_then(|r| r.as_u64());
                    if let Some(rgb_val) = rgb {
                        let hex = rgb_int_to_hex(rgb_val);
                        if query.contains("path:dersler") {
                            colors.insert("dersler".to_string(), hex);
                        } else if query.contains("path:hatalar") {
                            colors.insert("hatalar".to_string(), hex);
                        } else if query.contains("path:projeler") {
                            colors.insert("projeler".to_string(), hex);
                        } else if query.contains("file:sahip") {
                            colors.insert("sahip".to_string(), hex);
                        }
                    }
                }
            }
        }
    }
    (colors, show_arrow)
}

pub fn scan_vault(root: &Path) -> Result<GraphData, String> {
    let hafiza_dir = root.join("hafiza");
    if !hafiza_dir.exists() {
        return Err(format!("Hafıza klasörü bulunamadı: {}", hafiza_dir.display()));
    }
    let (custom_colors, show_arrow) = read_obsidian_graph_colors(&hafiza_dir);

    let mut files = Vec::new();
    collect_md_files(&hafiza_dir, &hafiza_dir, &mut files);
    files.sort_by(|a, b| a.0.cmp(&b.0));

    let link_re = wikilink_regex();

    // Map stem -> node_id (preferring exact stem)
    let mut stem_to_id: HashMap<String, String> = HashMap::new();
    let mut file_records: Vec<(String, String, String, PathBuf)> = Vec::new();

    for (rel, full) in files {
        let stem = Path::new(&rel)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or(&rel)
            .to_string();
        let id = rel.trim_end_matches(".md").to_string();
        stem_to_id.entry(stem.clone()).or_insert_with(|| id.clone());
        stem_to_id.insert(id.clone(), id.clone());
        file_records.push((id, stem, rel, full));
    }

    let mut edges: Vec<GraphEdge> = Vec::new();
    let mut edge_set: HashSet<(String, String)> = HashSet::new();
    let mut degree: HashMap<String, usize> = HashMap::new();

    for (id, _stem, _rel, full) in &file_records {
        let content = fs::read_to_string(full).unwrap_or_default();
        for cap in link_re.captures_iter(&content) {
            let target_raw = cap[1].trim();
            if let Some(target_id) = stem_to_id.get(target_raw) {
                if target_id != id && edge_set.insert((id.clone(), target_id.clone())) {
                    *degree.entry(id.clone()).or_insert(0) += 1;
                    *degree.entry(target_id.clone()).or_insert(0) += 1;
                    edges.push(GraphEdge {
                        source: id.clone(),
                        target: target_id.clone(),
                    });
                }
            }
        }
    }

    let mut nodes: Vec<GraphNode> = Vec::new();
    for (id, stem, rel, _full) in file_records {
        let folder = rel
            .split('/')
            .next()
            .filter(|_| rel.contains('/'))
            .unwrap_or("kök")
            .to_string();
        let (group, color) = classify_node(&rel, &stem, &custom_colors);
        nodes.push(GraphNode {
            degree: *degree.get(&id).unwrap_or(&0),
            id,
            label: stem,
            path: rel,
            folder,
            group,
            color,
        });
    }

    let groups = vec![
        GraphColorGroup {
            name: "Dersler".to_string(),
            color: custom_colors
                .get("dersler")
                .cloned()
                .unwrap_or_else(|| "#3b7ee6".to_string()),
        },
        GraphColorGroup {
            name: "Hatalar".to_string(),
            color: custom_colors
                .get("hatalar")
                .cloned()
                .unwrap_or_else(|| "#ef4444".to_string()),
        },
        GraphColorGroup {
            name: "Projeler".to_string(),
            color: custom_colors
                .get("projeler")
                .cloned()
                .unwrap_or_else(|| "#22c55e".to_string()),
        },
        GraphColorGroup {
            name: "Sahip & Kurallar".to_string(),
            color: custom_colors
                .get("sahip")
                .cloned()
                .unwrap_or_else(|| "#f59e0b".to_string()),
        },
        GraphColorGroup {
            name: "Genel".to_string(),
            color: "#94a3b8".to_string(),
        },
    ];

    Ok(GraphData {
        nodes,
        edges,
        groups,
        show_arrow,
    })
}

pub fn read_note(root: &Path, note_id: &str) -> Result<NoteContent, String> {
    let hafiza_dir = root.join("hafiza");
    // Prevent path traversal
    if note_id.contains("..") {
        return Err("Geçersiz not yolu".to_string());
    }
    let rel_path = if note_id.ends_with(".md") {
        note_id.to_string()
    } else {
        format!("{}.md", note_id)
    };
    let full = hafiza_dir.join(&rel_path);
    if !full.exists() {
        return Err(format!("Not bulunamadı: {}", rel_path));
    }
    let content = fs::read_to_string(&full).map_err(|e| e.to_string())?;
    let title = full
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(note_id)
        .to_string();
    let folder = rel_path
        .split('/')
        .next()
        .filter(|_| rel_path.contains('/'))
        .unwrap_or("kök")
        .to_string();

    let graph = scan_vault(root).unwrap_or(GraphData {
        nodes: vec![],
        edges: vec![],
        groups: vec![],
        show_arrow: true,
    });
    let clean_id = rel_path.trim_end_matches(".md").to_string();
    let links: Vec<String> = graph
        .edges
        .iter()
        .filter(|e| e.source == clean_id)
        .map(|e| e.target.clone())
        .collect();
    let backlinks: Vec<String> = graph
        .edges
        .iter()
        .filter(|e| e.target == clean_id)
        .map(|e| e.source.clone())
        .collect();

    Ok(NoteContent {
        id: clean_id,
        path: rel_path,
        title,
        folder,
        content,
        links,
        backlinks,
    })
}
