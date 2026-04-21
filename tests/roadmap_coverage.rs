use std::collections::{BTreeMap, BTreeSet};

use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct CoverageFile {
    item: Vec<CoverageItem>,
}

#[derive(Debug, Deserialize)]
struct CoverageItem {
    id: String,
    file: String,
    line: usize,
    text: String,
    status: String, // planned|done
    tests: Vec<String>,
    planned_tests: Vec<String>,
}

fn read_checklist_items() -> Vec<(String, usize, bool, String)> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir("docs/roadmaps").expect("docs/roadmaps exists") {
        let entry = entry.expect("dir entry");
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("md") {
            continue;
        }
        let file = path.file_name().unwrap().to_string_lossy().to_string();
        let text = std::fs::read_to_string(&path).expect("read roadmap");
        for (idx, line) in text.lines().enumerate() {
            let line_no = idx + 1;
            let trimmed = line.trim_start();
            if let Some(rest) = trimmed.strip_prefix("- [") {
                if let Some(end) = rest.find("] ") {
                    let mark = &rest[..end];
                    let payload = &rest[end + 2..];
                    if mark == " " || mark.eq_ignore_ascii_case("x") {
                        out.push((file.clone(), line_no, mark.eq_ignore_ascii_case("x"), payload.to_string()));
                    }
                }
            }
        }
    }
    out.sort();
    out
}

#[test]
fn roadmaps_have_complete_coverage_map() {
    let checklist = read_checklist_items();
    assert!(!checklist.is_empty(), "no checklist items found");

    let raw = std::fs::read_to_string("docs/roadmaps/coverage.toml")
        .expect("docs/roadmaps/coverage.toml must exist");
    let coverage: CoverageFile = toml::from_str(&raw).expect("coverage.toml parse");

    let mut by_key: BTreeMap<(String, usize, String), &CoverageItem> = BTreeMap::new();
    let mut ids = BTreeSet::new();
    for item in &coverage.item {
        assert!(ids.insert(item.id.clone()), "duplicate coverage id: {}", item.id);
        by_key.insert((item.file.clone(), item.line, item.text.clone()), item);
    }

    // Every checklist item must have an entry.
    let mut missing = Vec::new();
    for (file, line, _checked, text) in &checklist {
        if !by_key.contains_key(&(file.clone(), *line, text.clone())) {
            missing.push(format!("{file}:{line} {text}"));
        }
    }
    assert!(
        missing.is_empty(),
        "missing coverage entries (run generator or update coverage.toml):\n{}",
        missing.join("\n")
    );

    // No extra entries.
    let mut extra = Vec::new();
    for key in by_key.keys() {
        if !checklist
            .iter()
            .any(|(f, l, _c, t)| (f, l, t) == (&key.0, &key.1, &key.2))
        {
            extra.push(format!("{}:{} {}", key.0, key.1, key.2));
        }
    }
    assert!(
        extra.is_empty(),
        "extra coverage entries not present in roadmaps:\n{}",
        extra.join("\n")
    );

    // Enforce policy:
    // - Checked items must be status=done and have at least one test.
    // - Unchecked items must have at least one planned test name.
    let mut policy_violations = Vec::new();
    for (file, line, checked, text) in checklist {
        let item = by_key
            .get(&(file.clone(), line, text.clone()))
            .expect("checked above");
        if checked {
            if item.status != "done" {
                policy_violations.push(format!("{file}:{line} marked done but coverage.status={}", item.status));
            }
            if item.tests.is_empty() {
                policy_violations.push(format!("{file}:{line} marked done but coverage.tests is empty"));
            }
        } else if item.planned_tests.is_empty() && item.status != "done" {
            policy_violations.push(format!("{file}:{line} planned but coverage.planned_tests is empty"));
        }
    }
    assert!(
        policy_violations.is_empty(),
        "roadmap coverage policy violations:\n{}",
        policy_violations.join("\n")
    );
}

