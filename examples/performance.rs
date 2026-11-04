//! Reproducible CPU/data-path benchmark; does not claim UI frame-rate measurements.
use opengalgame::{authoring, catalog::*, model::*, project_io::*, runtime::*};
use std::{hint::black_box, time::Instant};
fn main() {
    let mut p = template_project("长篇性能验证 · 5000 节点", ProjectTemplate::Blank);
    p.nodes = (0..5000)
        .map(|i| {
            let mut n = Node::new(format!("scene_{i}"), NodeKind::Dialogue, i);
            n.title = format!("第 {} 场", i + 1);
            n.chapter = format!("第 {} 章", i / 100 + 1);
            n.text = "雨声落在窗前。这是用于长篇性能验证的中文剧本。".repeat(20);
            n.position = [32. + (i % 8) as f32 * 285., 32. + (i / 8) as f32 * 170.];
            if i == 4999 {
                n.kind = NodeKind::Ending;
            } else {
                n.next = Some(format!("scene_{}", i + 1));
            }
            n
        })
        .collect();
    p.start = "scene_0".into();
    let bytes = serde_json::to_vec(&p).unwrap();
    let start = Instant::now();
    let _: Project = serde_json::from_slice(&bytes).unwrap();
    let parse = start.elapsed();
    let start = Instant::now();
    let issues = validate(&p, None);
    assert!(issues.is_empty());
    let validation = start.elapsed();
    let start = Instant::now();
    for _ in 0..20 {
        for i in 0..5000 {
            black_box(p.node(&format!("scene_{i}"))).unwrap();
        }
    }
    let lookups = start.elapsed();
    let original = p.clone();
    let start = Instant::now();
    let mut history = vec![];
    for i in 0..100 {
        history.push(p.clone());
        p.nodes[i].text.push('新');
    }
    let edits = start.elapsed();
    let start = Instant::now();
    assert!(authoring::cycle_nodes(&p).is_empty());
    let cycles = start.elapsed();
    let mut runtime = Runtime::new(&p);
    for _ in 0..600 {
        runtime.advance(&p, None).unwrap();
    }
    let hash = fingerprint(&p);
    let start = Instant::now();
    let save = runtime.save_with_fingerprint(&p, &hash);
    let save_bytes = serde_json::to_vec(&save).unwrap();
    let saving = start.elapsed();
    assert_eq!(Runtime::load(&p, save).unwrap(), runtime);
    let metrics = serde_json::json!({"nodes":5000,"chinese_characters":p.nodes.iter().map(|n|n.text.chars().count()).sum::<usize>(),"project_json_bytes":bytes.len(),"parse_ms":parse.as_secs_f64()*1000.,"validate_ms":validation.as_secs_f64()*1000.,"indexed_100000_lookups_ms":lookups.as_secs_f64()*1000.,"100_undo_snapshots_and_edits_ms":edits.as_secs_f64()*1000.,"unchanged_nodes_shared_with_original":original.nodes.shared_with(&p.nodes),"unchanged_nodes_shared_with_last_snapshot":history.last().unwrap().nodes.shared_with(&p.nodes),"cycle_detection_ms":cycles.as_secs_f64()*1000.,"save_500_history_steps_ms":saving.as_secs_f64()*1000.,"save_json_bytes":save_bytes.len(),"scope":"release build CPU/data paths; excludes GPU rendering, audio and file-system latency"});
    println!("{}", serde_json::to_string_pretty(&metrics).unwrap());
    if let Some(path) = std::env::args().nth(1) {
        save_project(std::path::Path::new(&path), &p).unwrap();
    }
}
