//! Reproducible SDK packaging without ambient operating-system imports.

use std::{borrow::Cow, error::Error, path::Path};
use wasm_encoder::{ComponentNameSection, ComponentSection, CustomSection};
use wasmparser::{Parser, Payload};

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() != 3 {
        return Err("usage: cinnaroids-pack CORE.wasm COMPONENT.wasm NOTICES.txt".into());
    }
    let core = std::fs::read(&args[0])?;
    let mut component = wit_component::ComponentEncoder::default()
        .module(&core)?
        .validate(true)
        .encode()?;
    let mut name = ComponentNameSection::new();
    name.component("Cinnaroids");
    name.append_to_component(&mut component);
    append_section(
        &mut component,
        "version",
        env!("CARGO_PKG_VERSION").as_bytes(),
    );
    append_section(
        &mut component,
        "description",
        b"Clicker, aim assist, reach, FakeLag, Netherite Finder, Fullbright and Auto Jump Reset",
    );
    append_section(
        &mut component,
        "licenses",
        b"MIT AND PolyForm-Noncommercial-1.0.0",
    );
    append_section(
        &mut component,
        "cinnabar:grants",
        include_bytes!("../grants.json"),
    );
    append_section(
        &mut component,
        mod_api::ICON_SECTION,
        include_bytes!("../../../../assets/cinnaroids-icon.png"),
    );
    let notices = std::fs::read(&args[2])?;
    std::str::from_utf8(&notices)?;
    if notices.len() > mod_api::MAX_NOTICES_BYTES {
        return Err("Embedded notices exceed the host's size limit".into());
    }
    append_section(&mut component, mod_api::NOTICES_SECTION, &notices);
    wasmparser::Validator::new_with_features(wasmparser::WasmFeatures::all())
        .validate_all(&component)?;
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

/// Adds one top-level resource or metadata section to the finished component.
fn append_section(component: &mut Vec<u8>, name: &str, data: &[u8]) {
    CustomSection {
        name: Cow::Borrowed(name),
        data: Cow::Borrowed(data),
    }
    .append_to_component(component);
}
