use shipsim_core::shipyard;
use std::path::{Path, PathBuf};

fn main() {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() != 2 || !matches!(args[0].as_str(), "validate" | "cost" | "compile") {
        eprintln!("usage: shipsim-yard <validate|cost|compile> <design.toml>");
        std::process::exit(2);
    }
    let command = &args[0];
    let design = Path::new(&args[1]);
    let root = find_data_root(design).unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")));
    let result = match command.as_str() {
        "validate" => shipyard::validate(&root, design).map(|_| "valid".to_string()),
        "cost" => shipyard::design_cost(&root, design).map(|cost| cost.to_string()),
        "compile" => shipyard::compile(&root, design).map(|path| path.display().to_string()),
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
