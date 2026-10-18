#!/usr/bin/env python3
"""Generate the ClickGUI native surface and its retained transition bindings."""

import argparse
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / "mods/cinnaroids/ui/panel.json"

# Logical GUI units. The reference box is the area columns may be dragged within; the host
# scales the whole surface down when a viewport is smaller, so the box always stays visible.
REFERENCE = (760, 420)
COLUMN = 150
HEADER = 22
ROW = 20
# The footer matches the corner radius so square row tints never reach the rounded corners.
FOOTER = 5
PAD = 4096
INSET = 9

EXPO = [0.16, 1, 0.3, 1]
SETTLE = [0.2, 0.9, 0.25, 1]
LIFT = [0.34, 1.36, 0.64, 1]

# Control indices are declared in `State::panel_content`; tests assert both stay aligned.
MENU_KEY, RESET_LAYOUT = 0, 26
# Every header shares one screen-fixed drag pad backed by the two hidden sliders 27 and 28.
DRAG_PAD = 27
COLUMNS = [
    ("combat", "Combat", DRAG_PAD),
    ("network", "Network", DRAG_PAD),
    ("render", "Render", DRAG_PAD),
    ("menu", "Settings", DRAG_PAD),
]
# id, label, column, toggle, expand, key, settings
MODULES = [
    ("clicker", "Clicker", "combat", 1, 20, 3, [("slider", 2, "Clicks per second", "CPS")]),
    ("aim", "Aim assist", "combat", 4, 21, 7,
     [("slider", 5, "Strength", "%"), ("choice", 6, "Activation"),
      ("toggle", 29, "Horizontal only")]),
    ("reach", "Reach", "combat", 8, 22, 10, [("slider", 9, "Distance", "blocks")]),
    ("jump_reset", "Auto Jump Reset", "combat", 11, 23, 13,
     [("slider", 12, "Hit window", "ticks")]),
    ("fake_lag", "FakeLag", "network", 14, 24, None,
     [("slider", 15, "Delay", "ms"), ("toggle", 16, "Show real position")]),
    ("netherite", "Netherite Finder", "render", 17, 25, None,
     [("slider", 18, "Range", "blocks")]),
    ("fullbright", "Fullbright", "render", 19, None, None, []),
]
HEIGHTS = {"slider": 30, "choice": 32, "toggle": 20, "key": 22}


def rgba(hexadecimal, alpha=1.0):
    """Convert an sRGB palette value into native RGBA components."""
    return [round(int(hexadecimal[i:i + 2], 16) / 255, 4) for i in (0, 2, 4)] + [alpha]


TEXT = rgba("e6e8eb")
MUTED = rgba("a3a9b2")
ACCENT = rgba("ff8a65")
COLUMN_FILL = rgba("0f1115", 0.9)
DETAIL_FILL = rgba("000000", 0.24)
TINT = rgba("ff8a65", 0.17)
HOVER = rgba("ffffff", 0.05)
FIELD = rgba("ffffff", 0.06)
EDGE = rgba("ffffff", 0.13)
TRACK = rgba("ffffff", 0.13)
KNOB = rgba("f4f5f7")


def node(kind, size, offset=(0, 0), **properties):
    """Place a JSON-UI node relative to its parent's upper-left corner."""
    return dict(type=kind, size=list(size), offset=list(offset),
                anchor_from="top_left", anchor_to="top_left", **properties)


def children(**nodes):
    """Keep stable node names so scalar updates retain layout and pointer state."""
    return [{name: body} for name, body in nodes.items()]


def bind(target, *pairs):
    """Connect validated scalar bindings to native visual properties."""
    target["bindings"] = [dict(binding_name=name, binding_name_override=override)
                          for name, override in pairs]
    return target


def rect(size, offset=(0, 0), radius=0, color=EDGE, **properties):
    """Draw a bounded antialiased shape without external textures."""
    return node("custom", size, offset, renderer="cinnabar_rounded_rectangle",
                radius=radius, color=color, **properties)


def label(text, size, offset=(0, 0), *, color=TEXT, scale=0.9, align="left"):
    """Use the component's embedded Inter face for every label and value."""
    result = node("label", size, offset, text=text, text_alignment=align, color=color,
                  shadow=False, clips_children=True, hide_hyphen=True, localize=False,
                  font_type="mod_panel", font_scale_factor=scale)
    if text.startswith("#"):
        result["bindings"] = [dict(binding_name=text)]
    return result


def icon(name, size, offset, color=MUTED):
    """Use Cinnabar's vector icons, including the rotating disclosure chevron."""
    return node("custom", size, offset, renderer="cinnabar_vector_icon", icon=name, color=color)


def button(action, size, offset=(0, 0), hover=True, **contents):
    """Route one pointer press to a declared control, with eased native hover feedback."""
    route = f"mod.control:{action}" if isinstance(action, int) else action
    controls = {}
    if hover:
        feedback = rect(size, color=HOVER, alpha=0)
        if isinstance(action, int):
            bind(feedback, (f"#row_{action}_hover", "#alpha"))
        controls["feedback"] = feedback
    controls.update(contents)
    return node("button", size, offset, controls=children(**controls),
                button_mappings=[dict(from_button_id="button.menu_select",
                                      to_button_id=route, mapping_type="pressed")])


def keycap(index, name, width, offset):
    """Show a binding as a keycap; capture fades in a coral prompt over the same cap."""
    capture = node("panel", [width, 16], alpha=0, propagate_alpha=True, controls=children(
        background=rect([width, 16], radius=3, color=rgba("2a1d19"),
                        border_color=ACCENT, border_width=1),
        prompt=label("Press a key", [width, 10], [0, 4], color=ACCENT, scale=0.72,
                     align="center")))
    bind(capture, (f"#{name}_capture", "#alpha"))
    return button(index, [width, 16], offset, hover=False,
                  background=rect([width, 16], radius=3, color=FIELD, border_color=EDGE,
                                  border_width=1),
                  feedback=bind(rect([width, 16], radius=3, color=HOVER, alpha=0),
                                (f"#row_{index}_hover", "#alpha")),
                  key=label(f"#row_{index}_value", [width - 4, 10], [2, 4], scale=0.75,
                            align="center"),
                  capture=capture)


def switch(index, offset):
    """Ease a compact track color while native motion slides its knob."""
    track = rect([18, 10], [0, 0], 5, TRACK)
    active = bind(rect([18, 10], radius=5, color=ACCENT), (f"#row_{index}_toggle_progress", "#alpha"))
    knob = bind(rect([6, 6], [2, 2], 3, KNOB), (f"#row_{index}_knob_offset", "#offset"))
    track["controls"] = children(active=active, knob=knob)
    return button(index, [24, 16], offset, hover=False, track=node("panel", [18, 10], [3, 3],
                                                                    controls=children(track=track)))


def slider(index, title, unit, y):
    """Retain native dragging and typed numeric editing over the full field width."""
    width = COLUMN - 2 * INSET
    fill = bind(rect([width, 2], [0, 4], 1, ACCENT), (f"#row_{index}_fill", "#size_binding_x"))
    knob = rect([6, 6], radius=3, color=KNOB)
    knob.update(anchor_from="right_middle", anchor_to="center")
    fill["controls"] = children(knob=knob)
    # "%" sits against its number; word units keep a normal space.
    unit_width = 7 if unit == "%" else 3 + 4.4 * len(unit)
    gap = 0.4 if unit == "%" else 2
    return node("panel", [width, 26], [INSET, y], controls=children(
        title=label(title, [width - 50, 10], [0, 0], color=MUTED, scale=0.78),
        value=button(f"mod.edit:{index}", [44, 12], [width - 44, -1], hover=False,
                     text=label(f"#row_{index}_value", [44 - unit_width - gap, 10],
                                [0, 1], scale=0.78, align="right"),
                     unit=label(unit, [unit_width, 10], [44 - unit_width, 1], color=MUTED,
                                scale=0.72)),
        slider=button(index, [width, 10], [0, 13], hover=False,
                      track=rect([width, 2], [0, 4], 1, TRACK), fill=fill)))


def choice(index, title, y):
    """Use the host dropdown for keyboard and pointer choice selection."""
    width = COLUMN - 2 * INSET
    return node("panel", [width, 28], [INSET, y], controls=children(
        title=label(title, [width, 10], color=MUTED, scale=0.78),
        field=button(index, [width, 16], [0, 12], hover=False,
                     background=rect([width, 16], radius=3, color=FIELD),
                     feedback=bind(rect([width, 16], radius=3, color=HOVER, alpha=0),
                                   (f"#row_{index}_hover", "#alpha")),
                     value=label(f"#row_{index}_value", [width - 20, 10], [6, 4], scale=0.78),
                     chevron=icon("chevron", [5, 3], [width - 11, 7]))))


def toggle(index, title, y):
    """A labelled native switch inside a settings region."""
    width = COLUMN - 2 * INSET
    return node("panel", [width, 16], [INSET, y], controls=children(
        title=label(title, [width - 30, 10], [0, 4], color=MUTED, scale=0.78),
        switch=switch(index, [width - 22, 0])))


def binding_row(index, name, y):
    """Keep shortcut capture separate from enablement and disclosure."""
    width = COLUMN - 2 * INSET
    return node("panel", [width, 16], [INSET, y], controls=children(
        title=label("Bind", [40, 10], [0, 4], color=MUTED, scale=0.78),
        key=keycap(index, name, 58, [width - 58, 0])))


def details(module, settings, key):
    """Lay out one module's settings and report the region's full height."""
    controls, y = {}, 6
    for setting in settings:
        kind, index, title = setting[:3]
        if kind == "slider":
            controls[f"slider_{index}"] = slider(index, title, setting[3], y)
        elif kind == "choice":
            controls[f"choice_{index}"] = choice(index, title, y)
        else:
            controls[f"toggle_{index}"] = toggle(index, title, y)
        y += HEIGHTS[kind]
    if key is not None:
        controls["bind"] = binding_row(key, module, y)
        y += HEIGHTS["key"]
    height = y + 2
    content = bind(node("panel", [COLUMN, height], alpha=0, propagate_alpha=True,
                        controls=children(**controls)), (f"#{module}_detail", "#alpha"))
    region = node("panel", [COLUMN, 0], [0, ROW], clips_children=True, controls=children(
        background=rect(["100%", "100%"], color=DETAIL_FILL), content=content))
    return bind(region, (f"#{module}_h", "#size_binding_y_absolute")), height


def module_row(module, title, toggle_index, expand, key, settings):
    """A classic row: the label toggles; a separate arrow discloses the settings beneath."""
    width = COLUMN - (22 if expand is not None else 0)
    tint = bind(rect([COLUMN, ROW], color=TINT, alpha=0), (f"#row_{toggle_index}_toggle_progress", "#alpha"))
    bar = rect([2, ROW], color=ACCENT, alpha=0,
               cinnabar_transform={"scale": [1, 0], "origin": [0.5, 0.5]})
    bind(bar, (f"#row_{toggle_index}_toggle_progress", "#alpha"), (f"#{module}_bar", "#cinnabar_scale"))
    name = bind(label(title, [width - 14, 10], [10, 6], scale=0.92), (f"#{module}_indent", "#offset"))
    contents = dict(name=name)
    # The trailing value always reads in muted text; a bound key gets its own small keycap.
    # Bound modules slide the value left to make room for the keycap.
    if expand is not None:
        value = label(f"#{module}_value", [52, 10], [width - 56, 6], color=MUTED, scale=0.78,
                      align="right")
        if key is not None:
            value["bindings"].append(dict(binding_name=f"#{module}_value_at",
                                          binding_name_override="#offset"))
        contents["value"] = value
    if key is not None:
        legend = label(f"#{module}_bind", [20, 9], [0, 2.5], scale=0.66, align="center")
        legend["bindings"].append(dict(binding_name=f"#{module}_key_color",
                                       binding_name_override="#color"))
        keycap_node = bind(node("panel", [20, 12], [width - 22, 4], alpha=0, propagate_alpha=True,
                                controls=children(
                                    cap=rect([20, 12], radius=3, color=FIELD, border_color=EDGE,
                                             border_width=0.5),
                                    legend=legend)), (f"#{module}_keycap", "#alpha"))
        contents["keycap"] = keycap_node
    controls = dict(tint=tint, bar=bar, row=button(toggle_index, [width, ROW], **contents))
    height = 0
    if expand is not None:
        chevron = icon("chevron", [5, 3], [9, 9], MUTED)
        chevron["cinnabar_transform"] = {"rotation": -90}
        bind(chevron, (f"#{module}_angle", "#cinnabar_rotation"))
        controls["expand"] = button(expand, [22, ROW], [width, 0], chevron=chevron)
        controls["details"], height = details(module, settings, key)
    row = node("panel", [COLUMN, ROW], controls=children(**controls))
    return bind(row, (f"#{module}_y", "#offset")), height


def menu_rows():
    """Menu preferences: the reserved menu key and a layout reset."""
    width = COLUMN - 2 * INSET
    key = node("panel", [COLUMN, 22], [0, 4], controls=children(
        title=label("Menu key", [70, 10], [INSET, 7], scale=0.86),
        key=keycap(MENU_KEY, "menu", 58, [COLUMN - INSET - 58, 3])))
    reset = button(RESET_LAYOUT, [width, 18], [INSET, 30], hover=False,
                   background=rect([width, 18], radius=3, color=FIELD),
                   feedback=bind(rect([width, 18], radius=3, color=HOVER, alpha=0),
                                 (f"#row_{RESET_LAYOUT}_hover", "#alpha")),
                   name=label("Reset layout", [width, 10], [0, 5], scale=0.82, align="center"))
    return dict(menu_key=key, reset=reset), 52


def column(identifier, title, pad):
    """A draggable category column; the header grip maps the pointer across a fixed pad."""
    rows, heights = {}, []
    if identifier == "menu":
        rows, body = menu_rows()
        heights.append(body)
    else:
        for module, name, owner, toggle_index, expand, key, settings in MODULES:
            if owner == identifier:
                rows[module], height = module_row(module, name, toggle_index, expand, key, settings)
                heights.append(height)
    body = bind(node("panel", [COLUMN, 0], [0, HEADER], clips_children=True,
                     controls=children(**rows)), (f"#col_{identifier}_h", "#size_binding_y_absolute"))
    # Hover and press tints are rounded header shapes placed inside the screen-fixed pad, so
    # they follow the column and keep its corners; the header clip squares only the seam.
    def header_tint(alpha):
        tint = rect([COLUMN, HEADER], radius=5, color=rgba("ffffff", alpha))
        bind(tint, (f"#col_{identifier}_tint_h", "#size_binding_y_absolute"))
        return bind(node("panel", [COLUMN, HEADER], controls=children(tint=tint)),
                    (f"#col_{identifier}_tint_at", "#offset"))
    grip = button(f"mod.vector:{pad}:{pad + 1}", [PAD, PAD], hover=False,
                  idle=node("panel", [0, 0]), over=header_tint(0.04), held=header_tint(0.07))
    grip.update(default_control="idle", hover_control="over", pressed_control="held")
    bind(grip, (f"#col_{identifier}_pad", "#offset"))
    chevron = icon("chevron", [5, 3], [COLUMN - 14, 10])
    chevron["cinnabar_transform"] = {"rotation": 0}
    bind(chevron, (f"#col_{identifier}_angle", "#cinnabar_rotation"))
    # One translucent surface per column; a hairline divides the header, and the border
    # lifts while the column is dragged.
    header = dict(
        background=bind(rect([COLUMN, HEADER], radius=5, color=COLUMN_FILL,
                             border_color=rgba("ffffff", 0), border_width=0.5),
                        (f"#col_{identifier}_bg", "#size_binding_y_absolute"),
                        (f"#col_{identifier}_edge", "#border_color")),
        divider=bind(rect([COLUMN - 2 * INSET, 0.5], [INSET, HEADER - 0.5], color=rgba("ffffff", 0.09)),
                     (f"#col_{identifier}_open", "#alpha")),
        grip=node("panel", [COLUMN, HEADER], clips_children=True, controls=children(grip=grip)),
        title=label(title, [90, 11], [10, 5.5], scale=1.06),
        chevron=chevron,
        body=body)
    if identifier != "menu":
        header["count"] = label(f"#col_{identifier}_count", [36, 10], [COLUMN - 56, 7],
                                color=MUTED, scale=0.78, align="right")
    entrance = bind(node("panel", [COLUMN, HEADER], propagate_alpha=True,
                         controls=children(**header)),
                    (f"#col_{identifier}_in", "#offset"), (f"#col_{identifier}_alpha", "#alpha"))
    result = node("panel", [COLUMN, HEADER], controls=children(entrance=entrance))
    return bind(result, (f"#col_{identifier}_pos", "#offset")), heights


def document():
    """Floating columns over a dimmed, blurred world, centred on the reference box."""
    columns, heights = {}, {}
    for identifier, title, pad in COLUMNS:
        columns[identifier], heights[identifier] = column(identifier, title, pad)
    desk = node("panel", [0, 0], controls=children(**columns))
    desk.update(anchor_from="center", anchor_to="top_left")
    root = node("panel", ["100%", "100%"], controls=children(desk=desk))
    return dict(namespace="cinnaroids", panel=root), heights


def transitions():
    """Declare reversible open/close motion and eased disclosure on one retained route."""
    result = {
        # A symmetric curve holds the world dim until the faster column fade has finished,
        # so the columns never ghost over a cleared world on close.
        "#world_blur": dict(duration=0.38, easing="in_out_cubic", **{"from": 0, "exit": 0}),
        "#world_brightness": dict(duration=0.38, easing="in_out_cubic", **{"from": 1, "exit": 1}),
        "#world_saturation": dict(duration=0.38, easing="in_out_cubic", **{"from": 1, "exit": 1}),
    }
    for order, (identifier, _, _) in enumerate(COLUMNS):
        result[f"#col_{identifier}_in"] = dict(duration=0.3 + 0.045 * order, easing=EXPO,
                                               **{"from": [0, 10], "exit": [0, 5]})
        result[f"#col_{identifier}_alpha"] = dict(duration=0.14 + 0.02 * order, easing="out_cubic",
                                                  **{"from": 0, "exit": 0})
        result[f"#col_{identifier}_edge"] = dict(duration=0.12, easing="out_cubic")
        for prop in ["h", "bg"]:
            result[f"#col_{identifier}_{prop}"] = dict(duration=0.26, easing=EXPO)
        result[f"#col_{identifier}_open"] = dict(duration=0.16, easing="out_cubic")
        result[f"#col_{identifier}_angle"] = dict(duration=0.22, easing=SETTLE)
    for module, _, _, _, expand, key, _ in MODULES:
        result[f"#{module}_y"] = dict(duration=0.26, easing=EXPO)
        result[f"#{module}_bar"] = dict(duration=0.24, easing=LIFT)
        result[f"#{module}_indent"] = dict(duration=0.2, easing=SETTLE)
        if expand is not None:
            result[f"#{module}_h"] = dict(duration=0.26, easing=EXPO)
            result[f"#{module}_angle"] = dict(duration=0.22, easing=SETTLE)
            result[f"#{module}_detail"] = dict(duration=0.22, easing="out_cubic")
        if key is not None:
            result[f"#{module}_key_color"] = dict(duration=0.16, easing="out_cubic")
            result[f"#{module}_keycap"] = dict(duration=0.16, easing="out_cubic")
            result[f"#{module}_value_at"] = dict(duration=0.22, easing=SETTLE)
            result[f"#{module}_capture"] = dict(duration=0.14, easing="out_cubic")
    result["#menu_capture"] = dict(duration=0.14, easing="out_cubic")
    return result


def compact_document(document):
    """Share static node defaults so regular guest callbacks copy a small authored document."""
    bases = {}

    def compact(body):
        """Extract repeated flat presentation properties without changing names or behavior."""
        kind = body["type"]
        base = dict(anchor_from="top_left", anchor_to="top_left", offset=[0, 0])
        name = "base_" + kind
        if kind == "label":
            base.update(font_type="mod_panel", shadow=False, hide_hyphen=True,
                        localize=False, text_alignment="left", clips_children=True)
        if kind == "custom" and body.get("renderer") == "cinnabar_rounded_rectangle":
            colors = [key for key, value in bases.items()
                      if key.startswith("base_rect_") and value["color"] == body["color"]]
            name = colors[0] if colors else "base_rect_" + str(sum(key.startswith("base_rect_") for key in bases))
            base.update(renderer="cinnabar_rounded_rectangle", color=body["color"], radius=0)
        bases.setdefault(name, dict(type=kind, **base))
        del body["type"]
        for key, value in base.items():
            if body.get(key) == value:
                del body[key]
        for entry in body.get("controls", []):
            child_name, child_body = next(iter(entry.items()))
            child_base = compact(child_body)
            entry.clear()
            entry[child_name + "@" + child_base] = child_body
        return name

    # Retain the root's ordinary definition; all descendants use static local inheritance.
    root = document["panel"]
    for entry in root.get("controls", []):
        name, body = next(iter(entry.items()))
        base = compact(body)
        entry.clear()
        entry[name + "@" + base] = body
    document.update(bases)
    return document


def main():
    """Regenerate the deterministic native surface or verify its checked-in freshness."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    authored, _ = document()
    text = json.dumps(dict(screen="cinnaroids.panel",
                           document=json.dumps(compact_document(authored), separators=(",", ":")),
                           transitions=transitions()), separators=(",", ":"))
    if args.check:
        if not OUTPUT.exists() or OUTPUT.read_text() != text:
            raise SystemExit(f"{OUTPUT.relative_to(ROOT)} needs regeneration")
    else:
        OUTPUT.write_text(text)


if __name__ == "__main__":
    main()
