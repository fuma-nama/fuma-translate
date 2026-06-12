use std::path::Path;
use std::time::Instant;

use rayon::prelude::*;
use rustc_hash::FxHashSet;

use crate::{analyze_file, collect_files, compile_files, FileAnalysis};

const BASIC_PATTERN: &str = "/tmp/fuma-translate-bench/files/*.tsx";
const LARGE_PATTERN: &str = "/tmp/fuma-translate-bench-large/files/*.tsx";

fn bench_compile_sync(label: &str, pattern: &str) {
    let sample = pattern.trim_end_matches("*.tsx");
    if !Path::new(sample).is_dir() {
        return;
    }

    let t0 = Instant::now();
    let output = compile_files(&[pattern.to_string()])
        .unwrap_or_else(|error| panic!("compile_files: {}", error.message));
    let total_ms = t0.elapsed().as_millis();

    eprintln!(
        "{label} compile_sync: {} unique keys in {total_ms}ms",
        output.translation_keys.len()
    );
}

fn bench_pattern(label: &str, pattern: &str) {
    let sample = pattern.trim_end_matches("*.tsx");
    if !Path::new(sample).is_dir() {
        eprintln!(
            "skip {label}: missing {sample}\n\
             run: packages/core/test/bench/setup.sh"
        );
        return;
    }

    let t0 = Instant::now();
    let files = match collect_files(&[pattern.to_string()]) {
        Ok(files) => files,
        Err(error) => panic!("collect_files: {}", error.message),
    };
    let collect_ms = t0.elapsed().as_millis();

    let t1 = Instant::now();
    let analyses: Vec<FileAnalysis> = files
        .par_iter()
        .map(|path| analyze_file(path.as_path()))
        .collect();
    let analyze_ms = t1.elapsed().as_millis();

    let t2 = Instant::now();
    let mut keys = FxHashSet::default();
    for analysis in analyses {
        keys.extend(analysis.keys);
    }
    let merge_ms = t2.elapsed().as_millis();

    eprintln!("{label}: {} files, {} keys", files.len(), keys.len());
    eprintln!("  collect: {collect_ms}ms");
    eprintln!("  analyze: {analyze_ms}ms");
    eprintln!("  merge: {merge_ms}ms");
    eprintln!("  total: {}ms", collect_ms + analyze_ms + merge_ms);
}

/// Generate inputs with `packages/core/test/bench/setup.sh`, then run from repo root:
/// `cargo test --release bench -- --ignored --nocapture`
#[test]
#[ignore = "manual benchmark; requires generated files in /tmp"]
fn compile_phases() {
    bench_pattern("basic", BASIC_PATTERN);
    bench_pattern("large", LARGE_PATTERN);
    bench_compile_sync("basic", BASIC_PATTERN);
    bench_compile_sync("large", LARGE_PATTERN);
}
