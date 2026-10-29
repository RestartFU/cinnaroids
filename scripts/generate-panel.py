#!/usr/bin/env python3
"""Generate the ClickGUI panel surface and the gameplay HUD surface for Player ESP labels."""

import argparse
import json
import struct
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / "mods/cinnaroids/ui/panel.json"
HUD_OUTPUT = ROOT / "mods/cinnaroids/ui/hud.json"
FONT = ROOT / "assets/fonts/Inter-Medium.ttf"
# Cinnabar lays out the embedded panel font at nine GUI units per em.
FONT_EM = 9

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
# The component publishes every control's feedback itself: `#c<index>_on` eases a toggle,
# `#c<index>_text` shows a slider, keybind or choice value and `#c<index>_fill` sizes a slider.
MENU_KEY, RESET_LAYOUT = 0, 26
# Every header shares one screen-fixed drag pad backed by the two hidden sliders 27 and 28.
DRAG_PAD = 27
COLUMNS = [
    ("combat", "Combat", DRAG_PAD),
    ("network", "Network", DRAG_PAD),
    ("render", "Render", DRAG_PAD),
    ("menu", "Settings", DRAG_PAD),
]
# id, label, column, toggle, expand, key, settings. The first slider also supplies the
# row's trailing value; Reach publishes its own fixed-decimal value instead.
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
    ("player_esp", "Player ESP", "render", 30, 36, None,
     [("slider", 31, "Range", "blocks"), ("choice", 32, "Outline color"),
      ("toggle", 33, "Names"), ("toggle", 34, "Distance"), ("toggle", 35, "Health")]),
    ("auto_totem", "AutoTotem", "combat", 37, None, None, []),
]
HEIGHTS = {"slider": 30, "choice": 32, "toggle": 20, "key": 22}


def advances():
    """Read the embedded font's horizontal advances by character, in em fractions."""
    data = FONT.read_bytes()
    tables = {}
    for index in range(struct.unpack(">H", data[4:6])[0]):
        tag, _, offset, _ = struct.unpack(">4sIII", data[12 + 16 * index:28 + 16 * index])
        tables[tag.decode()] = offset
    units = struct.unpack(">H", data[tables["head"] + 18:tables["head"] + 20])[0]
    metrics = struct.unpack(">H", data[tables["hhea"] + 34:tables["hhea"] + 36])[0]
    cmap = tables["cmap"]
    glyphs = {}
    for index in range(struct.unpack(">H", data[cmap + 2:cmap + 4])[0]):
        platform, encoding, offset = struct.unpack(">HHI", data[cmap + 4 + 8 * index:cmap + 12 + 8 * index])
        table = cmap + offset
        if (platform, encoding) not in ((3, 1), (0, 3)) or struct.unpack(">H", data[table:table + 2])[0] != 4:
            continue
        segments = struct.unpack(">H", data[table + 6:table + 8])[0] // 2
        arrays = [struct.unpack(f">{segments}H", data[start:start + 2 * segments])
                  for start in (table + 14, table + 16 + 2 * segments,
                                table + 16 + 4 * segments, table + 16 + 6 * segments)]
        ends, starts, deltas, ranges = arrays
        for segment in range(segments):
            for code in range(starts[segment], min(ends[segment], 0x7f) + 1):
                if ranges[segment] == 0:
                    glyph = (code + deltas[segment]) & 0xffff
                else:
                    at = table + 16 + 6 * segments + 2 * segment + ranges[segment] + 2 * (code - starts[segment])
                    glyph = struct.unpack(">H", data[at:at + 2])[0]
                    glyph = (glyph + deltas[segment]) & 0xffff if glyph else 0
                glyphs[chr(code)] = glyph
        break
    hmtx = tables["hmtx"]

    def advance(glyph):
        glyph = min(glyph, metrics - 1)
        return struct.unpack(">H", data[hmtx + 4 * glyph:hmtx + 4 * glyph + 2])[0] / units

    return {character: advance(glyph) for character, glyph in glyphs.items()}


ADVANCES = advances()


def text_width(text, scale):
    """Width of one unkerned label line in the embedded font, in GUI units."""
    return round(sum(ADVANCES[character] for character in text) * FONT_EM * scale, 2)


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


def states(size, radius=0):
    """Hover and press tints the native button shows only while it is in that state."""
    return dict(hover=rect(size, radius=radius, color=HOVER, visible=False),
                press=rect(size, radius=radius, color=HOVER, visible=False))


def button(action, size, offset=(0, 0), feedback=True, **contents):
    """Route one pointer press to a declared control, with native hover and press feedback."""
    route = f"mod.control:{action}" if isinstance(action, int) else action
    controls = states(size) if feedback else {}
    controls.update(contents)
    result = node("button", size, offset, controls=children(**controls),
                  button_mappings=[dict(from_button_id="button.menu_select",
                                        to_button_id=route, mapping_type="pressed")])
    if "hover" in controls:
        result.update(hover_control="hover", pressed_control="press")
    return result


def keycap(index, name, width, offset):
    """Show a binding as a keycap; capture fades in a coral prompt over the same cap."""
    capture = node("panel", [width, 16], alpha=0, propagate_alpha=True, controls=children(
        background=rect([width, 16], radius=3, color=rgba("2a1d19"),
                        border_color=ACCENT, border_width=1),
        prompt=label("Press a key", [width, 10], [0, 4], color=ACCENT, scale=0.72,
                     align="center")))
    bind(capture, (f"#{name}_capture", "#alpha"))
    return button(index, [width, 16], offset, feedback=False,
                  background=rect([width, 16], radius=3, color=FIELD, border_color=EDGE,
                                  border_width=1),
                  **states([width, 16], 3),
                  key=label(f"#c{index}_text", [width - 4, 10], [2, 4], scale=0.75,
                            align="center"),
                  capture=capture)


def switch(index, offset):
    """One eased toggle progress fades the coral track in and slides the knob across."""
    track = rect([18, 10], [0, 0], 5, TRACK)
    active = bind(rect([18, 10], radius=5, color=ACCENT), (f"#c{index}_on", "#alpha"))
    # The travel grows across the 10-unit lane; the knob rides its right edge.
    knob = rect([6, 6], radius=3, color=KNOB)
    knob.update(anchor_from="right_middle", anchor_to="left_middle")
    travel = bind(node("panel", [0, 6], controls=children(knob=knob)),
                  (f"#c{index}_on", "#size_binding_x"))
    lane = node("panel", [10, 6], [2, 2], controls=children(travel=travel))
    track["controls"] = children(active=active, lane=lane)
    return button(index, [24, 16], offset, feedback=False, track=node("panel", [18, 10], [3, 3],
                                                                    controls=children(track=track)))


def slider(index, title, unit, y):
    """Retain native dragging and typed numeric editing over the full field width."""
    width = COLUMN - 2 * INSET
    fill = bind(rect([width, 2], [0, 4], 1, ACCENT), (f"#c{index}_fill", "#size_binding_x"))
    knob = rect([6, 6], radius=3, color=KNOB)
    knob.update(anchor_from="right_middle", anchor_to="center")
    fill["controls"] = children(knob=knob)
    # "%" sits against its number; word units keep a normal space.
    unit_width = 7 if unit == "%" else 3 + 4.4 * len(unit)
    gap = 0.4 if unit == "%" else 2
    return node("panel", [width, 26], [INSET, y], controls=children(
        title=label(title, [width - 50, 10], [0, 0], color=MUTED, scale=0.78),
        value=button(f"mod.edit:{index}", [44, 12], [width - 44, -1], feedback=False,
                     text=label(f"#c{index}_text", [44 - unit_width - gap, 10],
                                [0, 1], scale=0.78, align="right"),
                     unit=label(unit, [unit_width, 10], [44 - unit_width, 1], color=MUTED,
                                scale=0.72)),
        slider=button(index, [width, 10], [0, 13], feedback=False,
                      track=rect([width, 2], [0, 4], 1, TRACK), fill=fill)))


def choice(index, title, y):
    """Use the host dropdown for keyboard and pointer choice selection."""
    width = COLUMN - 2 * INSET
    return node("panel", [width, 28], [INSET, y], controls=children(
        title=label(title, [width, 10], color=MUTED, scale=0.78),
        field=button(index, [width, 16], [0, 12], feedback=False,
                     background=rect([width, 16], radius=3, color=FIELD),
                     **states([width, 16], 3),
                     value=label(f"#c{index}_text", [width - 20, 10], [6, 4], scale=0.78),
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


def trailing_value(module, settings):
    """The row's muted value: the first slider's number and a fixed unit, ending at the right."""
    if module == "reach":
        return label(f"#{module}_value", [52, 10], color=MUTED, scale=0.78, align="right")
    _, index, _, unit = settings[0]
    unit = unit if unit == "%" else " " + unit
    width = text_width(unit, 0.78)
    return node("panel", [52, 10], controls=children(
        number=label(f"#c{index}_text", [52 - width, 10], color=MUTED, scale=0.78,
                     align="right"),
        unit=label(unit, [width + 4, 10], [48 - width, 0], color=MUTED, scale=0.78,
                   align="right")))


def module_row(module, title, toggle_index, expand, key, settings, first):
    """A classic row: the label toggles; a separate arrow discloses the settings beneath."""
    width = COLUMN - (22 if expand is not None else 0)
    progress = f"#c{toggle_index}_on"
    tint = bind(rect([COLUMN, ROW], color=TINT, alpha=0), (progress, "#alpha"))
    bar = rect([2, ROW], color=ACCENT, alpha=0,
               cinnabar_transform={"scale": [1, 0], "origin": [0.5, 0.5]})
    bind(bar, (progress, "#alpha"), (f"#{module}_bar", "#cinnabar_scale"))
    name = bind(label(title, [width - 14, 10], [10, 6], scale=0.92), (f"#{module}_indent", "#offset"))
    contents = dict(name=name)
    # The trailing value always reads in muted text; a bound key gets its own small keycap.
    # Bound modules slide the value left to make room for the keycap.
    if expand is not None:
        value = trailing_value(module, settings)
        value["offset"] = [width - 56, 6]
        if key is not None:
            value.setdefault("bindings", []).append(
                dict(binding_name=f"#{module}_value_at", binding_name_override="#offset"))
        contents["value"] = value
    if key is not None:
        # Enabled rows draw the legend in coral over the same Bone legend.
        legend = label(f"#c{key}_text", [20, 9], [0, 2.5], scale=0.66, align="center")
        lit = label(f"#c{key}_text", [20, 9], [0, 2.5], color=ACCENT, scale=0.66, align="center")
        lit["bindings"].append(dict(binding_name=progress, binding_name_override="#alpha"))
        keycap_node = bind(node("panel", [20, 12], [width - 22, 4], alpha=0, propagate_alpha=True,
                                controls=children(
                                    cap=rect([20, 12], radius=3, color=FIELD, border_color=EDGE,
                                             border_width=0.5),
                                    legend=legend, lit=lit)), (f"#{module}_keycap", "#alpha"))
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
    # A column's first row never moves, so it needs no live offset.
    return (row if first else bind(row, (f"#{module}_y", "#offset"))), height


def menu_rows():
    """Menu preferences: the reserved menu key and a layout reset."""
    width = COLUMN - 2 * INSET
    key = node("panel", [COLUMN, 22], [0, 4], controls=children(
        title=label("Menu key", [70, 10], [INSET, 7], scale=0.86),
        key=keycap(MENU_KEY, "menu", 58, [COLUMN - INSET - 58, 3])))
    reset = button(RESET_LAYOUT, [width, 18], [INSET, 30], feedback=False,
                   background=rect([width, 18], radius=3, color=FIELD),
                   **states([width, 18], 3),
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
                rows[module], height = module_row(module, name, toggle_index, expand, key,
                                                  settings, not rows)
                heights.append(height)
    body = bind(node("panel", [COLUMN, 0], [0, HEADER], clips_children=True,
                     controls=children(**rows)), (f"#col_{identifier}_h", "#size_binding_y_absolute"))
    # Hover and press tints are rounded header shapes placed inside the screen-fixed pad, so
    # they follow the column and keep its corners; the header clip squares only the seam.
    # The pad is centred on the desk origin and the tints sit at the column position from
    # its centre. Tints share the plate height: taller than the header while open, so the
    # clip squares the seam, and exactly the header while collapsed, so corners stay round.
    def header_tint(alpha):
        tint = rect([COLUMN, HEADER], radius=5, color=rgba("ffffff", alpha))
        bind(tint, (f"#col_{identifier}_bg", "#size_binding_y_absolute"))
        tint_at = node("panel", [COLUMN, HEADER], controls=children(tint=tint))
        tint_at.update(anchor_from="center")
        return bind(tint_at, (f"#col_{identifier}_pos", "#offset"))
    grip = button(f"mod.vector:{pad}:{pad + 1}", [PAD, PAD], feedback=False,
                  idle=node("panel", [0, 0]), over=header_tint(0.04), held=header_tint(0.07))
    grip.update(default_control="idle", hover_control="over", pressed_control="held",
                anchor_to="center")
    bind(grip, (f"#col_{identifier}_pad", "#offset"))
    chevron = icon("chevron", [5, 3], [COLUMN - 14, 10])
    chevron["cinnabar_transform"] = {"rotation": 0}
    bind(chevron, (f"#col_{identifier}_angle", "#cinnabar_rotation"))
    # One translucent surface per column; a hairline divides the header while the column is
    # open, and the border lifts while the column is dragged.
    divider = bind(rect([COLUMN - 2 * INSET, 0.5], [INSET, HEADER - 0.5],
                        color=rgba("ffffff", 0.09)),
                   (f"#col_{identifier}_open", "#alpha"))
    header = dict(
        background=bind(rect([COLUMN, HEADER], radius=5, color=COLUMN_FILL,
                             border_color=rgba("ffffff", 0), border_width=0.5),
                        (f"#col_{identifier}_bg", "#size_binding_y_absolute"),
                        (f"#col_{identifier}_edge", "#border_color")),
        divider=divider,
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
    firsts = set()
    for module, _, owner, toggle, expand, key, settings in MODULES:
        if owner in firsts:
            result[f"#{module}_y"] = dict(duration=0.26, easing=EXPO)
        firsts.add(owner)
        result[f"#{module}_bar"] = dict(duration=0.24, easing=LIFT)
        result[f"#{module}_indent"] = dict(duration=0.2, easing=SETTLE)
        if expand is not None:
            result[f"#{module}_h"] = dict(duration=0.26, easing=EXPO)
            result[f"#{module}_angle"] = dict(duration=0.22, easing=SETTLE)
            result[f"#{module}_detail"] = dict(duration=0.22, easing="out_cubic")
        if key is not None:
            result[f"#{module}_keycap"] = dict(duration=0.16, easing="out_cubic")
            result[f"#{module}_value_at"] = dict(duration=0.22, easing=SETTLE)
            result[f"#{module}_capture"] = dict(duration=0.14, easing="out_cubic")
    result["#menu_capture"] = dict(duration=0.14, easing="out_cubic")
    # Toggle feedback eases briefly, as Cinnabar's native controls did.
    for module, _, _, toggle, _, _, settings in MODULES:
        for index in [toggle] + [s[1] for s in settings if s[0] == "toggle"]:
            result[f"#c{index}_on"] = dict(duration=0.1, easing="out_cubic")
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


# Player ESP labels; matches `esp::MAX_PLAYERS`.
ESP_LABELS = 6
# Label plates keep the earlier host world-label style: 1.2 scale, 2-unit padding, no shadow
# and a 65% black background.
ESP_SCALE = 1.2
ESP_PADDING = 2 * ESP_SCALE
# One line of label text, as `esp.rs` sizes plates.
ESP_LINE = 10 * ESP_SCALE


def hud_document():
    """Centre one dark name plate on each projected label point; the root takes no input."""
    slots = {}
    for index in range(ESP_LABELS):
        # Surface text holds no line breaks, so each of the two lines is its own label.
        lines = {}
        for line in (1, 2):
            name = f"#esp_{index}_line{line}"
            lines[f"line{line}"] = node(
                "label", [f"100% - {2 * ESP_PADDING}px", ESP_LINE],
                [ESP_PADDING, ESP_PADDING + (line - 1) * ESP_LINE], text=name,
                bindings=[dict(binding_name=name)], localize=False, shadow=False,
                font_scale_factor=0.8 * ESP_SCALE, color=[1, 1, 1, 1], text_alignment="center")
        plate = bind(node("panel", [0, 0], controls=children(
            background=rect(["100%", "100%"], color=[0, 0, 0, 0.65]), **lines)),
            (f"#esp_{index}_w", "#size_binding_x_absolute"),
            (f"#esp_{index}_h", "#size_binding_y_absolute"))
        plate["anchor_to"] = "center"
        point = bind(node("panel", [0, 0], controls=children(plate=plate)),
                     (f"#esp_{index}_at", "#offset"), (f"#esp_{index}_at_visible", "#visible"))
        slots[f"esp_{index}"] = bind(node("panel", ["100%", "100%"], controls=children(point=point)),
                                     (f"#esp_{index}_shown", "#visible"))
    root = node("panel", ["100%", "100%"], controls=children(**slots))
    return dict(namespace="cinnaroids", hud=root)


def hud_surface():
    """The HUD layer with every label hidden until the first update."""
    bindings = {}
    for index in range(ESP_LABELS):
        bindings[f"#esp_{index}_shown"] = False
        for line in (1, 2):
            bindings[f"#esp_{index}_line{line}"] = ""
    return dict(screen="cinnaroids.hud",
                document=json.dumps(hud_document(), separators=(",", ":")), bindings=bindings)


def main():
    """Regenerate the deterministic surfaces or verify their checked-in freshness."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    authored, _ = document()
    outputs = {
        OUTPUT: json.dumps(dict(screen="cinnaroids.panel",
                                document=json.dumps(compact_document(authored),
                                                    separators=(",", ":")),
                                transitions=transitions()), separators=(",", ":")),
        HUD_OUTPUT: json.dumps(hud_surface(), separators=(",", ":")),
    }
    for output, text in outputs.items():
        if args.check:
            if not output.exists() or output.read_text() != text:
                raise SystemExit(f"{output.relative_to(ROOT)} needs regeneration")
        else:
            output.write_text(text)


if __name__ == "__main__":
    main()
