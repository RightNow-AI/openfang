//! Build automation tasks for the OpenFang workspace.
//!
//! Usage:
//!   cargo xtask openapi-gen              — print OpenAPI 3.1 JSON to stdout
//!   cargo xtask openapi-gen --out PATH   — write to a file path
fn main() {
    let mut args = std::env::args().skip(1);
    match args.next().as_deref() {
        Some("openapi-gen") => {
            use utoipa::OpenApi as _;
            let doc = openfang_api::openapi::ApiDoc::openapi();
            let json =
                serde_json::to_string_pretty(&doc).expect("OpenAPI serialization should not fail");

            // Optional --out <path> flag
            let out_path = {
                let mut out: Option<String> = None;
                let mut rest = args;
                while let Some(arg) = rest.next() {
                    if arg == "--out" {
                        out = rest.next();
                        break;
                    }
                }
                out
            };

            match out_path {
                Some(path) => {
                    std::fs::write(&path, &json)
                        .unwrap_or_else(|e| panic!("Failed to write {path}: {e}"));
                    eprintln!("xtask: OpenAPI spec written to {path}");
                }
                None => println!("{json}"),
            }
        }
        Some(cmd) => {
            eprintln!("xtask: unknown command '{cmd}'");
            std::process::exit(1);
        }
        None => {
            eprintln!("xtask commands:");
            eprintln!("  openapi-gen [--out <path>]   Generate OpenAPI 3.1 JSON spec");
        }
    }
}
