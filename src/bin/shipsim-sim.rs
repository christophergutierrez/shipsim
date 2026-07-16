use std::env;
use std::path::PathBuf;
use std::process::ExitCode;

use shipsim_core::simulation::{run_suite, SuiteSpec};

struct Args {
    suite: PathBuf,
    output: Option<PathBuf>,
}

fn main() -> ExitCode {
    match run() {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::from(2),
        Err(error) => {
            eprintln!("{error}");
            ExitCode::from(1)
        }
    }
}

fn run() -> Result<bool, String> {
    let args = parse_args(env::args().skip(1))?;
    let text = std::fs::read_to_string(&args.suite)
        .map_err(|error| format!("cannot read suite {:?}: {error}", args.suite))?;
    let mut spec: SuiteSpec =
        toml::from_str(&text).map_err(|error| format!("cannot parse suite: {error}"))?;
    resolve_paths(&args.suite, &mut spec);
    let report = match run_suite(&spec) {
        Ok(report) => report,
        Err(error) => {
            if let Some(failure) = error.failed_match() {
                let json = serde_json::to_vec_pretty(failure)
                    .map_err(|serialize_error| serialize_error.to_string())?;
                if let Some(path) = args.output.as_ref() {
                    if let Some(parent) = path.parent() {
                        std::fs::create_dir_all(parent).map_err(|io_error| {
                            format!("cannot create output directory {parent:?}: {io_error}")
                        })?;
                    }
                    std::fs::write(path, json)
                        .map_err(|io_error| format!("cannot write report {path:?}: {io_error}"))?;
                    eprintln!("failed match report: {}", path.display());
                } else {
                    eprintln!("{}", String::from_utf8(json).expect("JSON is UTF-8"));
                }
            }
            return Err(error.to_string());
        }
    };
    let json = serde_json::to_vec_pretty(&report).map_err(|error| error.to_string())?;
    if let Some(path) = args.output {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|error| format!("cannot create output directory {parent:?}: {error}"))?;
        }
        std::fs::write(&path, &json)
            .map_err(|error| format!("cannot write report {path:?}: {error}"))?;
        println!("report: {}", path.display());
    } else {
        println!("{}", String::from_utf8(json).expect("JSON is UTF-8"));
    }
    println!(
        "matches={} termination_rate={:.3} decided_equivalent_rate={:.3} capped_matches={} win_rate={:.3} average_turns={:.2}",
        report.aggregate.matches,
        report.aggregate.termination_rate,
        report.aggregate.decided_equivalent_rate,
        report.aggregate.capped_matches,
        report.aggregate.win_rate,
        report.aggregate.average_turns
    );
    for rubric in &report.rubrics {
        println!(
            "rubric {}: {}",
            rubric.id,
            if rubric.passed { "PASS" } else { "FAIL" }
        );
    }
    Ok(report
        .rubrics
        .iter()
        .all(|rubric| rubric.passed || !rubric.blocking))
}

fn resolve_paths(suite_path: &std::path::Path, spec: &mut SuiteSpec) {
    let root = suite_path
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."));
    // Suite lives at simulation/suites/foo.toml → repo root is two parents up.
    let repo_root = suite_path
        .parent()
        .and_then(|p| p.parent())
        .and_then(|p| p.parent())
        .unwrap_or(root);
    if let Some(scenario) = spec.scenario.as_ref() {
        if scenario.is_relative() && !scenario.exists() {
            let from_cwd = scenario.clone();
            let from_suite = root.join(scenario);
            let from_repo = repo_root.join(scenario);
            spec.scenario = Some(if from_cwd.exists() {
                from_cwd
            } else if from_suite.exists() {
                from_suite
            } else {
                from_repo
            });
        }
    }
    if spec.data_root.is_none() {
        spec.data_root = Some(repo_root.to_path_buf());
    } else if let Some(dr) = spec.data_root.as_ref() {
        if dr.is_relative() && !dr.exists() {
            spec.data_root = Some(repo_root.join(dr));
        }
    }
    for rubric in &mut spec.rubrics {
        if rubric.is_relative() && !rubric.exists() {
            let candidate = root.join(&*rubric);
            *rubric = if candidate.exists() {
                candidate
            } else {
                repo_root.join(&*rubric)
            };
        }
    }
}

fn parse_args(args: impl IntoIterator<Item = String>) -> Result<Args, String> {
    let mut suite = None;
    let mut output = None;
    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--suite" => {
                suite = Some(PathBuf::from(
                    iter.next()
                        .ok_or_else(|| "--suite requires a path".to_string())?,
                ));
            }
            "--output" => {
                output = Some(PathBuf::from(
                    iter.next()
                        .ok_or_else(|| "--output requires a path".to_string())?,
                ));
            }
            _ => return Err(format!("unknown argument {arg:?}")),
        }
    }
    Ok(Args {
        suite: suite.ok_or_else(|| "--suite is required".to_string())?,
        output,
    })
}
