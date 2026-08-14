use shipsim_core::shipyard;
use std::path::{Path, PathBuf};

fn main() {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let valid = (args.len() == 1 && args[0] == "check-all")
        || (args.len() == 2
            && matches!(args[0].as_str(), "validate" | "cost" | "compile" | "check"));
    if !valid {
        eprintln!("usage: shipsim-yard <validate|cost|compile|check> <design.toml>");
        eprintln!("       shipsim-yard check-all");
        std::process::exit(2);
    }
    let command = &args[0];
    let design = args.get(1).map(Path::new);
    let root = design
        .and_then(find_data_root)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")));
    let result = match command.as_str() {
        "validate" => {
            shipyard::validate(&root, design.expect("validated args")).map(|_| "valid".to_string())
        }
        "cost" => shipyard::design_cost(&root, design.expect("validated args"))
            .map(|cost| cost.to_string()),
        "compile" => shipyard::compile(&root, design.expect("validated args"))
            .map(|path| path.display().to_string()),
        "check" => shipyard::check(&root, design.expect("validated args"))
            .map(|path| path.display().to_string()),
        "check-all" => {
            shipyard::check_all(&root).map(|count| format!("checked {count} yard designs"))
        }
        _ => unreachable!("command validated above"),
    };
    match result {
        Ok(value) => println!("{value}"),
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}

fn find_data_root(design: &Path) -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if let Ok(path) = design.canonicalize() {
        candidates.extend(path.ancestors().map(Path::to_path_buf));
    }
    if let Ok(cwd) = std::env::current_dir() {
        candidates.extend(cwd.ancestors().map(Path::to_path_buf));
    }
    candidates.into_iter().find(|candidate| {
        candidate.join("data/components.toml").is_file()
            && candidate.join("data/sizes.toml").is_file()
    })
}
