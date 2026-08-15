//! Print a deterministic standard-yard budget roster for use before a fight.

use shipsim_core::simulation::{build_budget_fleet, BudgetPolicy};
use std::path::Path;

fn usage() -> ! {
    eprintln!("usage: shipsim-budget --budget N --policy largest|swarm|balance --roster id,id,...");
    std::process::exit(2);
}

fn main() {
    let mut budget = None;
    let mut policy = None;
    let mut roster = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--budget" => budget = args.next().and_then(|value| value.parse().ok()),
            "--policy" => policy = args.next(),
            "--roster" => roster = args.next(),
            _ => usage(),
        }
    }
    let budget = budget.unwrap_or_else(|| usage());
    let policy = match policy.as_deref() {
        Some("largest") => BudgetPolicy::Largest,
        Some("swarm") => BudgetPolicy::Swarm,
        Some("balance") => BudgetPolicy::Balance,
        _ => usage(),
    };
    let roster: Vec<String> = roster
        .unwrap_or_else(|| usage())
        .split(',')
        .filter(|id| !id.is_empty())
        .map(str::to_owned)
        .collect();
    let fleet =
        build_budget_fleet(Path::new("."), budget, policy, &roster).unwrap_or_else(|error| {
            eprintln!("budget error: {error}");
            std::process::exit(1);
        });
    println!("# deterministic budget roster; use once per side");
    for line in fleet {
        println!("class={} count={}", line.class, line.count);
    }
}
