//! Reproducible SDK packaging without ambient operating-system imports.

use std::{error::Error, path::Path};
use wasmparser::{Parser, Payload};

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() != 2 {
        return Err("usage: cinnaroids-pack CORE.wasm COMPONENT.wasm".into());
    }
    let core = std::fs::read(&args[0])?;
    let component = wit_component::ComponentEncoder::default()
        .module(&core)?
        .validate(true)
        .encode()?;
    // Reject accidental ambient OS dependencies even if the component validates.
    for payload in Parser::new(0).parse_all(&component) {
        match payload? {
            Payload::ImportSection(imports) => {
                for import in imports.into_imports() {
                    let import = import?;
                    if import.module.starts_with("wasi") {
                        return Err("Cinnaroids must not import WASI".into());
                    }
                }
            }
            Payload::ComponentImportSection(imports) => {
                for import in imports {
                    if import?.name.0.starts_with("wasi") {
                        return Err("Cinnaroids must not import WASI".into());
                    }
                }
            }
            _ => {}
        }
    }
    let output = Path::new(&args[1]);
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(output, &component)?;
    println!("Validated component: {} bytes, no WASI", component.len());
    Ok(())
}
