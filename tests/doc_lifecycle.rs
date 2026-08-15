//! Guard: ephemeral working documents must not be tracked in git.
//!
//! Policy and rationale: `docs/DOC-LIFECYCLE.md`.
//!
//! A plan, handoff, review, or dated working file is true only until the work
//! is done. Tracking them leaves later readers unable to tell which documents
//! still apply. Keep them under `tmp/` (git-ignored) instead.

use std::path::Path;
use std::process::Command;

/// Name fragments that mark a file as ephemeral working output.
const EPHEMERAL_MARKERS: &[&str] = &[
    "-PLAN",
    "HANDOFF",
    "FINDINGS",
    "RECOMMENDATIONS",
    "VERDICT",
    "INVENTORY",
    "-LOG",
    "MILESTONES",
    "REMEDIATION",
    "PLAYTEST",
    "REVIEW",
];

/// Durable files whose names would otherwise trip a marker.
///
/// Every entry needs a reason. An allowlist without justification is how the
/// rot comes back.
const ALLOWLIST: &[(&str, &str)] = &[
    (
        "docs/BALANCE-PROTOCOL.md",
        "durable protocol: how balance evidence is gathered, not a task list",
    ),
    (
        "docs/UI-PLAYTEST-PROTOCOL.md",
        "durable protocol: how to run a playtest, reusable across runs",
    ),
    (
        "docs/history/BALANCE-CAMPAIGN-2026-07.md",
        "deliberate archive under docs/history/, explicitly labelled historical",
    ),
];

fn repo_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

fn tracked_files() -> Vec<String> {
    let out = Command::new("git")
        .arg("ls-files")
        .current_dir(repo_root())
        .output()
        .expect("run `git ls-files`");
    assert!(out.status.success(), "git ls-files failed");
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(str::to_string)
        .collect()
}

fn allowlisted(path: &str) -> bool {
    ALLOWLIST.iter().any(|(p, _)| *p == path)
}

#[test]
fn no_ephemeral_documents_are_tracked() {
    let mut offenders = Vec::new();
    for path in tracked_files() {
        // Policy covers *documents*. Source files are out of scope: `preview`
        // legitimately contains "review", and code is not a working document.
        if !path.ends_with(".md") {
            continue;
        }
        if allowlisted(&path) {
            continue;
        }
        // `docs/plans/` is ephemeral by definition, whatever the file is named.
        if path.starts_with("docs/plans/") {
            offenders.push(format!("{path}  (docs/plans/ is ephemeral by policy)"));
            continue;
        }
        let upper = path.to_uppercase();
        let name = upper.rsplit('/').next().unwrap_or(&upper).to_string();
        if let Some(marker) = EPHEMERAL_MARKERS.iter().find(|m| name.contains(**m)) {
            offenders.push(format!("{path}  (matches `{marker}`)"));
        }
    }

    assert!(
        offenders.is_empty(),
        "Ephemeral documents are tracked in git:\n  {}\n\n\
         These are true only until their work is done. Move them under `tmp/` \
         (git-ignored) and `git rm` them.\n\
         If a file is durable, rename it to describe what the system *is* \
         rather than what someone will *do*, or add it to ALLOWLIST in \
         tests/doc_lifecycle.rs with a reason.\n\
         Before deleting, repoint inbound references:\n  \
         git grep -l \"THE-DOC.md\" -- '*.md' '*.rs' '*.py' '*.lua'\n\
         Policy: docs/DOC-LIFECYCLE.md",
        offenders.join("\n  ")
    );
}

/// A dated working file (`-20260714`) is ephemeral regardless of its name.
#[test]
fn no_date_stamped_working_files_are_tracked() {
    let stamped: Vec<_> = tracked_files()
        .into_iter()
        .filter(|p| p.ends_with(".md") && !allowlisted(p))
        .filter(|p| {
            let name = p.rsplit('/').next().unwrap_or(p);
            // `-YYYYMMDD` anywhere in the stem.
            name.as_bytes().windows(9).any(|w| {
                w[0] == b'-' && w[1..].iter().all(u8::is_ascii_digit)
            })
        })
        .collect();

    assert!(
        stamped.is_empty(),
        "Date-stamped working documents are tracked:\n  {}\n\n\
         A dated file is a snapshot of a moment, not a description of the \
         system. Move it to `tmp/`, or to `docs/history/` if it is a \
         deliberate archive (and allowlist it).\n\
         Policy: docs/DOC-LIFECYCLE.md",
        stamped.join("\n  ")
    );
}

/// Durable directories must not accumulate ephemeral scratch, **tracked or not**.
///
/// The tracked-file checks above have a blind spot: an untracked working
/// document can sit in `docs/` indefinitely and no git-based check will see it.
/// That is exactly how a stale `docs/HANDOFF.md` survived — untracked, ignored
/// by the guard, and still the first thing a reader saw in the directory.
/// `docs/` is where durable material lives; scratch belongs under `tmp/`.
#[test]
fn durable_directories_contain_no_ephemeral_scratch() {
    fn walk(dir: &Path, out: &mut Vec<String>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, out);
            } else if path.extension().and_then(|e| e.to_str()) == Some("md") {
                if let Ok(rel) = path.strip_prefix(repo_root()) {
                    out.push(rel.to_string_lossy().replace('\\', "/"));
                }
            }
        }
    }

    let mut found = Vec::new();
    walk(&repo_root().join("docs"), &mut found);

    let offenders: Vec<_> = found
        .into_iter()
        .filter(|p| !allowlisted(p))
        .filter(|p| {
            let upper = p.to_uppercase();
            let name = upper.rsplit('/').next().unwrap_or(&upper).to_string();
            EPHEMERAL_MARKERS.iter().any(|m| name.contains(*m))
        })
        .collect();

    assert!(
        offenders.is_empty(),
        "Ephemeral scratch is sitting in a durable directory:\n  {}\n\n\
         These may be untracked, which is why the git-based checks do not see \
         them. `docs/` is for durable material; move working files to `tmp/`.\n\
         Policy: docs/DOC-LIFECYCLE.md",
        offenders.join("\n  ")
    );
}
