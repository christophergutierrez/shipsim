use std::fs;
use std::path::{Path, PathBuf};

fn has_size_threshold(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    let Some(size) = lower.find("size") else {
        return false;
    };
    let rest = &line[size + 4..];
    [">=", "<=", "==", ">", "<"].iter().any(|operator| {
        let Some(index) = rest.find(operator) else {
            return false;
        };
        if operator.len() == 1 && index > 0 && rest.as_bytes()[index - 1] == b'=' {
            return false;
        }
        let after = rest[index + operator.len()..].trim_start();
        after
            .chars()
            .next()
            .is_some_and(|ch| ch.is_ascii_digit() || ch.is_ascii_uppercase() || ch == '_')
    })
}

fn is_rule_duplication_hit(lines: &[&str], index: usize) -> bool {
    let start = index.saturating_sub(1);
    let end = (index + 1).min(lines.len().saturating_sub(1));
    let window = lines[start..=end].join(" ").to_ascii_lowercase();
    has_size_threshold(lines[index]) && ["repair", "cap", "power", "cost"]
        .iter()
        .any(|term| window.contains(term))
}

fn has_escape_marker(line: &str) -> bool {
    line.contains("derived-from-snapshot: false")
}

fn detector(text: &str) -> bool {
    let lines: Vec<_> = text.lines().collect();
    lines.iter().enumerate().any(|(index, line)| {
        is_rule_duplication_hit(&lines, index)
            && !has_escape_marker(line)
            && (index == 0 || !has_escape_marker(lines[index - 1]))
    })
}

fn tracked_source_files() -> Vec<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    for directory in ["frontend/repl", "frontend/love", "frontend/tui/src"] {
        collect_sources(&root.join(directory), &mut files);
    }
    files
}

fn collect_sources(path: &Path, files: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(path) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if path.file_name().is_some_and(|name| name == "tools") {
                continue;
            }
            collect_sources(&path, files);
        } else if path.extension().is_some_and(|ext| {
            matches!(ext.to_str(), Some("py" | "lua" | "rs"))
        }) {
            files.push(path);
        }
    }
}

#[test]
fn detector_matches_historical_repl_bug() {
    assert!(detector("cap = 2 if d.size >= 5 else 1"));
    assert!(detector("if size >= 5 { repair = 2 } else { repair = 1 }"));
    assert!(!detector("// derived-from-snapshot: false\ncap = 2 if d.size >= 5 else 1"));
    assert!(!detector("widths.sort_by(|a,b| a.size.cmp(&b.size))"));
}

#[test]
fn live_frontends_have_no_unmarked_hits() {
    let mut hits = Vec::new();
    for path in tracked_source_files() {
        let text = fs::read_to_string(&path).expect("read frontend source");
        let lines: Vec<_> = text.lines().collect();
        for (index, line) in lines.iter().enumerate() {
            if is_rule_duplication_hit(&lines, index)
                && !has_escape_marker(line)
                && (index == 0 || !has_escape_marker(lines[index - 1]))
            {
                hits.push(format!("{}:{}: {}", path.display(), index + 1, line));
            }
        }
    }
    assert!(hits.is_empty(), "client rule-duplication hits:\n{}", hits.join("\n"));
}
