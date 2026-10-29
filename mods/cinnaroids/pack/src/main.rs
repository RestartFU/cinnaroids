//! Reproducible SDK packaging without ambient operating-system imports.

use std::{borrow::Cow, error::Error, path::Path};
use wasm_encoder::{ComponentNameSection, ComponentSection, CustomSection};
use wasmparser::{Parser, Payload};

/// Optional release discovery metadata, readable without granting mod network access.
const UPDATE_REPOSITORY_SECTION: &str = "cinnabar:extension/update-repository@1";
/// Exact single-file release asset for hosts supporting automatic mod updates.
const UPDATE_URL_SECTION: &str = "cinnabar:extension/update-url@1";

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
    let asset = Path::new(&args[1])
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("Component output must have a UTF-8 file name")?;
    append_release_metadata(&mut component, asset);
    append_section(
        &mut component,
        "description",
        b"Clicker, aim assist, reach, AutoTotem, FakeLag, Player ESP, Netherite Finder, Fullbright and Auto Jump Reset",
    );
    append_section(
        &mut component,
        "licenses",
        b"MIT AND PolyForm-Noncommercial-1.0.0 AND OFL-1.1",
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
    let font = include_bytes!("../../../../assets/fonts/Inter-Medium.ttf");
    if font.len() > mod_api::MAX_PANEL_FONT_BYTES {
        return Err("Embedded panel font exceeds the host's size limit".into());
    }
    append_section(&mut component, mod_api::PANEL_FONT_SECTION, font);
    let bundle = panel_bundle()?;
    append_section(&mut component, mod_api::PANEL_BUNDLE_SECTION, &bundle);
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

/// Adds the workspace version, update repository and exact latest-release asset at the root.
fn append_release_metadata(component: &mut Vec<u8>, asset: &str) {
    append_section(component, "version", env!("CARGO_PKG_VERSION").as_bytes());
    append_section(
        component,
        UPDATE_REPOSITORY_SECTION,
        env!("CARGO_PKG_REPOSITORY").as_bytes(),
    );
    append_section(component, UPDATE_URL_SECTION, update_url(asset).as_bytes());
}

/// Derives the stable download URL from the workspace repository and packaged asset name.
fn update_url(asset: &str) -> String {
    format!(
        "{}/releases/latest/download/{asset}",
        env!("CARGO_PKG_REPOSITORY")
    )
}

/// Adds one top-level resource or metadata section to the finished component.
fn append_section(component: &mut Vec<u8>, name: &str, data: &[u8]) {
    CustomSection {
        name: Cow::Borrowed(name),
        data: Cow::Borrowed(data),
    }
    .append_to_component(component);
}

/// Embed the immutable route so frame callbacks send only controls and scalar values.
fn panel_bundle() -> Result<Vec<u8>, serde_json::Error> {
    use cinnaroids_mod::state::{Preferences, State};
    let panel: serde_json::Value =
        serde_json::from_str(&State::new(Preferences::default()).panel_json()?)?;
    serde_json::to_vec(&serde_json::json!({
        "panel": panel,
        "surfaces": { "clickgui": panel["surface"] }
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packaged_release_metadata_is_present_once_at_the_component_root() {
        let mut component = wasm_encoder::Component::new().finish();
        append_release_metadata(&mut component, "cinnaroids.wasm");
        wasmparser::Validator::new_with_features(wasmparser::WasmFeatures::all())
            .validate_all(&component)
            .unwrap();
        let metadata: Vec<_> = Parser::new(0)
            .parse_all(&component)
            .filter_map(|payload| match payload.unwrap() {
                Payload::CustomSection(section) => {
                    Some((section.name().to_owned(), section.data().to_vec()))
                }
                _ => None,
            })
            .collect();
        assert_eq!(
            metadata,
            vec![
                (
                    "version".into(),
                    env!("CARGO_PKG_VERSION").as_bytes().to_vec()
                ),
                (
                    UPDATE_REPOSITORY_SECTION.into(),
                    env!("CARGO_PKG_REPOSITORY").as_bytes().to_vec()
                ),
                (
                    UPDATE_URL_SECTION.into(),
                    update_url("cinnaroids.wasm").into_bytes()
                ),
            ]
        );
        assert!(env!("CARGO_PKG_REPOSITORY").starts_with("https://github.com/"));
    }
}
