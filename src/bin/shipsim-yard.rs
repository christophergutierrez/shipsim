use shipsim_core::shipyard;
use std::path::Path;

fn main() {
    let mut args = std::env::args().skip(1);
    let command = args.next().unwrap_or_default();
    let design = args.next().unwrap_or_default();
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let result = match command.as_str() {
        "validate" => shipyard::validate(root, Path::new(&design)).map(|_| "valid".to_string()),
        "cost" => shipyard::design_cost(root, Path::new(&design)).map(|cost| cost.to_string()),
        "compile" => {
            shipyard::compile(root, Path::new(&design)).map(|path| path.display().to_string())
        }
        _ => Err(shipyard::Error::InvalidId(
            "usage: shipsim-yard <validate|cost|compile> <design.toml>".into(),
        )),
    };
    match result {
        Ok(value) => println!("{value}"),
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}
