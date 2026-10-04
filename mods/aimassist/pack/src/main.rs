//! Reproducible SDK packaging with a validated, unique desktop configuration.

use std::{error::Error, path::Path};
use wasmparser::{Parser, Payload};

const MAGIC: &[u8; 16] = b"CNBR_AIM_CFG_v1!";

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() != 2 {
        return Err("usage: cinnabar-aimassist-pack CORE.wasm COMPONENT.wasm".into());
    }
    let core = std::fs::read(&args[0])?;
    let component = wit_component::ComponentEncoder::default()
        .module(&core)?
        .validate(true)
        .encode()?;
    let offsets: Vec<_> = component
        .windows(MAGIC.len())
        .enumerate()
        .filter_map(|(offset, bytes)| (bytes == MAGIC).then_some(offset))
        .collect();
    if offsets.len() != 1 {
        return Err(format!("expected one config marker, found {}", offsets.len()).into());
    }
    let offset = offsets[0] + MAGIC.len();
    if component.get(offset..offset + 8) != Some(&[0, 35, 1, 0, 0, 0, 0, 0]) {
        return Err("missing or invalid initial configuration payload".into());
    }
    // Reject accidental ambient OS dependencies even if the component validates.
    for payload in Parser::new(0).parse_all(&component) {
        match payload? {
            Payload::ImportSection(imports) => {
                for import in imports.into_imports() {
                    let import = import?;
                    if import.module.starts_with("wasi") {
                        return Err("aim assist must not import WASI".into());
                    }
                }
            }
            Payload::ComponentImportSection(imports) => {
                for import in imports {
                    if import?.name.0.starts_with("wasi") {
                        return Err("aim assist must not import WASI".into());
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
    println!(
        "Validated component: {} bytes, one config marker, payload offset {}, no WASI",
        component.len(),
        offset
    );
    Ok(())
}
