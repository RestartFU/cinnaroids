//! One native scroll view containing every module's square card.

/// Copy already serialized geometry so panel updates fit the host's callback fuel.
/// The host supplies live widget values independently through its row bindings.
pub(super) fn append(json: &mut String, dark: bool) {
    json.pop();
    json.push_str(",\"surface\":");
    json.push_str(if dark {
        include_str!("../ui/panel-dark.json")
    } else {
        include_str!("../ui/panel-light.json")
    });
    json.push('}');
}

#[cfg(test)]
mod tests;
