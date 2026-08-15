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
