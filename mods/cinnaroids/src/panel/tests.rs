use serde_json::Value;

/// Collect native action routes throughout the authored surface.
fn actions(node: &Value, result: &mut Vec<String>) {
    if let Some(mappings) = node["button_mappings"].as_array() {
        for mapping in mappings {
            result.push(mapping["to_button_id"].as_str().unwrap().to_owned());
        }
    }
    if let Some(controls) = node["controls"].as_array() {
        for entry in controls {
            for child in entry.as_object().unwrap().values() {
                actions(child, result);
            }
        }
    }
}

/// Resolve a named child without coupling assertions to draw order.
fn child<'a>(parent: &'a Value, name: &str) -> &'a Value {
    parent["controls"]
        .as_array()
        .unwrap()
        .iter()
        .find_map(|entry| entry.get(name))
        .unwrap()
}

#[test]
fn both_themes_keep_every_control_on_one_scrollable_square_card_grid() {
    for dark in [true, false] {
        let mut json = "{\"controls\":[]}".to_owned();
        super::append(&mut json, dark);
        let panel: Value = serde_json::from_str(&json).unwrap();
        let document: Value =
            serde_json::from_str(panel["surface"]["document"].as_str().unwrap()).unwrap();
        let root = &document["panel"];
        let dialog = child(root, "dialog");
        let view = child(dialog, "modules");
        assert_eq!(view["type"], "scroll_view");
        let viewport = child(view, "viewport");
        assert_eq!(viewport["clips_children"], true);
        let cards = child(viewport, "cards");
        let entries = cards["controls"].as_array().unwrap();
        assert_eq!(entries.len(), 8);
        for entry in entries {
            let card = entry.as_object().unwrap().values().next().unwrap();
            assert_eq!(card["size"], serde_json::json!([132, 132]));
        }
        let mut routes = Vec::new();
        actions(root, &mut routes);
        for index in 0..22 {
            assert_eq!(
                routes
                    .iter()
                    .filter(|route| **route == format!("mod.control:{index}"))
                    .count(),
                1,
                "control {index} must appear exactly once"
            );
        }
        for index in [2, 5, 9, 12, 15, 19] {
            assert!(routes.contains(&format!("mod.edit:{index}")));
        }
        assert!(
            !routes.iter().any(|route| route.starts_with("mod.category:")
                || matches!(route.as_str(), "mod.next" | "mod.prev"))
        );
        assert!(routes.contains(&"mod.close".to_owned()));
    }
}
