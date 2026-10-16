#!/usr/bin/env python3
"""Generate Fold's native accordion surfaces and their retained transition bindings."""

import argparse
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
WIDTH = 370
ROW = 32
EASE = [0.16, 1, 0.3, 1]
MODULES = [
    ("clicker", "Clicker", 1, 3, 22, 48),
    ("aim", "Aim assist", 4, 7, 23, 80),
    ("reach", "Reach", 8, 10, 24, 48),
    ("jump_reset", "Auto Jump Reset", 11, 13, 25, 48),
    ("fake_lag", "FakeLag", 14, None, 26, 68),
    ("netherite", "Netherite Finder", 18, None, 27, 48),
    ("fullbright", "Fullbright", 20, None, None, 0),
]


def node(kind, size, offset=(0, 0), **properties):
    """Place a JSON-UI node relative to its parent's upper-left corner."""
    return dict(type=kind, size=list(size), offset=list(offset),
                anchor_from="top_left", anchor_to="top_left", **properties)


def children(**nodes):
    """Keep stable node names so scalar updates retain layout and pointer state."""
    return [{name: body} for name, body in nodes.items()]


def binding(name, target):
    """Connect a validated scalar binding to one native visual property."""
    return dict(binding_name=name, binding_name_override=target)


def color(hexadecimal):
    """Convert the approved palette's sRGB values into native RGBA components."""
    return [round(int(hexadecimal[i:i + 2], 16) / 255, 4) for i in (0, 2, 4)] + [1]


class Surface:
    """Keep Fold's single-column rhythm in both persisted appearance modes."""

    def __init__(self, dark):
        self.outer = color("201b23" if dark else "f3eded")
        self.inset = color("302630" if dark else "e9dfe3")
        self.hover = color("3b303b" if dark else "dfd1d8")
        self.border = color("4d3e4b" if dark else "cbb7c2")
        self.text = color("f1ece9" if dark else "3d2838")
        self.muted = color("c0b3bb" if dark else "76576b")
        self.accent = color("ff9a85" if dark else "a84239")
        self.track = color("8c7487" if dark else "ad8e9f")

    def rect(self, size, offset=(0, 0), radius=0, color=None):
        """Draw a bounded antialiased shape without external textures."""
        return node("custom", size, offset, renderer="cinnabar_rounded_rectangle",
                    radius=radius, color=self.border if color is None else color)

    def label(self, text, size, offset=(0, 0), *, muted=False, scale=0.85, align="left"):
        """Use the component's embedded smooth Inter face for labels and values."""
        result = node("label", size, offset, text=text, text_alignment=align,
                      color=self.muted if muted else self.text, shadow=False,
                      clips_children=True, hide_hyphen=True, localize=False,
                      font_type="mod_panel", font_scale_factor=scale)
        if text.startswith("#"):
            result["bindings"] = [dict(binding_name=text)]
        return result

    def icon(self, name, size, offset, *, muted=False):
        """Use Cinnabar's vector icons, including the rotating disclosure chevron."""
        return node("custom", size, offset, renderer="cinnabar_vector_icon",
                    icon=name, color=self.muted if muted else self.accent)

    def button(self, index, size, offset=(0, 0), **contents):
        """Route one pointer action to its stable declared native control index."""
        action = f"mod.control:{index}" if isinstance(index, int) else index
        feedback = self.rect(size, radius=3, color=self.hover)
        if isinstance(index, int):
            feedback["bindings"] = [binding(f"#row_{index}_hover", "#alpha")]
            feedback["alpha"] = 0
        else:
            feedback["alpha"] = 0
        return node("button", size, offset, controls=children(feedback=feedback, **contents),
                    button_mappings=[dict(from_button_id="button.menu_select",
                                          to_button_id=action, mapping_type="pressed")])

    def toggle(self, index, offset):
        """Ease the approved track colors while native motion slides its knob."""
        track = self.rect([20, 10], [0, 2], 5, self.track)
        active = self.rect([20, 10], radius=5, color=self.accent)
        active["bindings"] = [binding(f"#row_{index}_toggle_progress", "#alpha")]
        knob = self.rect([6, 6], [2, 2], 3, self.outer)
        knob["bindings"] = [binding(f"#row_{index}_knob_offset", "#offset")]
        track["controls"] = children(active=active, knob=knob)
        return self.button(index, [20, 14], offset, track=track)

    def key(self, index, offset, width=34):
        """Keep shortcut capture separate from disclosure and module enablement."""
        return self.button(index, [width, 18], offset,
                           border=self.rect([width, 18], radius=3),
                           background=self.rect([width - 1, 17], [0.5, 0.5], 2.5, self.outer),
                           text=self.label(f"#row_{index}_value", [width - 2, 13], [1, 4],
                                           muted=True, scale=0.7, align="center"))

    def slider(self, index, title, y, unit):
        """Retain native dragging and typed numeric editing over the full field width."""
        width = WIDTH - 56
        fill = self.rect([width, 2], [0, 4], 1, self.accent)
        fill["bindings"] = [binding(f"#row_{index}_fill", "#size_binding_x")]
        knob = self.rect([6, 6], radius=3, color=self.accent)
        knob.update(anchor_from="right_middle", anchor_to="center")
        fill["controls"] = children(knob=knob)
        return node("panel", [width, 38], [28, y], controls=children(
            label=self.label(title, [200, 13], muted=True),
            value=self.button(f"mod.edit:{index}", [58, 14], [width - 80, 0],
                              text=self.label(f"#row_{index}_value", [58, 13], align="right")),
            unit=self.label(unit, [21, 13], [width - 21, 0], muted=True, scale=0.7),
            slider=self.button(index, [width, 14], [0, 17],
                               track=self.rect([width, 2], [0, 4], 1, self.track), fill=fill)))

    def choice(self, index, y):
        """Use the host dropdown for keyboard and pointer choice selection."""
        width = WIDTH - 56
        return node("panel", [width, 31], [28, y], controls=children(
            label=self.label("Activation", [width, 12], muted=True),
            choice=self.button(index, [width, 20], [0, 13],
                               background=self.rect([width, 20], radius=3, color=self.outer),
                               value=self.label(f"#row_{index}_value", [width - 20, 14], [6, 5], scale=0.8),
                               chevron=self.icon("chevron", [5, 3], [width - 12, 9], muted=True))))

    def details(self, module):
        """Keep all settings in a clipped region whose height eases to or from zero."""
        content = {
            "clicker": dict(cps=self.slider(2, "Clicks per second", 8, "CPS")),
            "aim": dict(strength=self.slider(5, "Strength", 8, "%"), activation=self.choice(6, 44)),
            "reach": dict(distance=self.slider(9, "Distance", 8, "blocks")),
            "jump_reset": dict(window=self.slider(12, "Hit window", 8, "ticks")),
            "fake_lag": dict(delay=self.slider(15, "Delay", 8, "ms"),
                             real_label=self.label("Show real position", [220, 14], [28, 48], scale=0.8),
                             real_toggle=self.toggle(16, [WIDTH - 48, 48])),
            "netherite": dict(range=self.slider(19, "Search range", 8, "blocks")),
        }[module]
        region = node("panel", [WIDTH, 0], [0, ROW], clips_children=True,
                      controls=children(background=self.rect(["100%", "100%"], color=self.inset), **content))
        region["bindings"] = [binding(f"#fold_{module}_height", "#size_binding_y_absolute")]
        return region

    def module(self, name, title, toggle, key, disclosure, _height):
        """Show a summary and independent controls on every compact module row."""
        title_x = 28 if disclosure is not None else 14
        contents = dict(title=self.label(title, [240, 15], [title_x, 5], scale=0.9),
                        summary=self.label(f"#summary_{name}", [250, 12], [title_x, 19], muted=True, scale=0.7))
        if disclosure is not None:
            chevron = self.icon("chevron", [5, 3], [12, 15], muted=True)
            chevron["cinnabar_transform"] = {"rotation": -90}
            chevron["bindings"] = [binding(f"#fold_{name}_angle", "#cinnabar_rotation")]
            contents["chevron"] = chevron
            row = self.button(disclosure, [WIDTH - (96 if key else 56), ROW], **contents)
        else:
            row = node("panel", [WIDTH - 56, ROW], controls=children(**contents))
        controls = dict(row=row, toggle=self.toggle(toggle, [WIDTH - 34, 10]),
                        rule=self.rect([WIDTH - 28, 0.5], [14, 0]))
        if key is not None:
            controls["key"] = self.key(key, [WIDTH - 82, 7], 36)
        if disclosure is not None:
            controls["details"] = self.details(name)
        result = node("panel", [WIDTH, ROW], controls=children(**controls))
        result["bindings"] = [binding(f"#fold_{name}_offset", "#offset")]
        return result

    def document(self):
        """Keep header and emergency controls fixed around the one scrolling accordion."""
        modules = node("panel", [WIDTH, 304], controls=children(**{
            args[0]: self.module(*args) for args in MODULES}))
        modules["bindings"] = [binding("#fold_content_height", "#size_binding_y_absolute")]
        track = node("scroll_track", [3, "100%"], controls=children(
            background=self.rect(["100%", "100%"], radius=1.5, color=self.outer),
            thumb=node("scrollbar_box", ["100%", "100%"], draggable="vertical", controls=children(
                background=self.rect(["100%", "100%"], radius=1.5, color=self.track)))))
        view = node("scroll_view", [WIDTH, "100% - 88px"], [0, 44],
                    scroll_content="module_rows", scroll_view_port="viewport", scrollbar_track="track",
                    scrollbar_box="thumb", scroll_box_and_track_panel="scrollbar", scroll_speed=24,
                    controls=children(
                        viewport=node("panel", [WIDTH, "100%"], clips_children=True,
                                      controls=children(module_rows=modules)),
                        scrollbar=node("panel", [3, "100%"], [WIDTH - 5, 0], controls=children(track=track))))
        footer = node("panel", [WIDTH, 44], controls=children(
            rule=self.rect([WIDTH, 0.5]),
            count=self.label("#enabled_count", [140, 15], [14, 8], muted=True, scale=0.7),
            stop=self.button(21, [77, 18], [211, 4],
                             text=self.label("Disable all  F10", [77, 14], [0, 4], muted=True, scale=0.7)),
            menu_label=self.label("Menu", [24, 14], [295, 8], muted=True, scale=0.7),
            menu=self.key(0, [323, 4], 32),
            hint=self.label("#binding_hint", [WIDTH - 28, 14], [14, 28], muted=True, scale=0.65)))
        footer.update(anchor_from="bottom_left", anchor_to="top_left", offset=[0, -44])
        settings = self.button(28, [WIDTH - 20, 52], [10, 44],
                               background=self.rect([WIDTH - 20, 52], radius=5, color=self.inset),
                               title=self.label("Menu settings", [150, 15], [12, 6], scale=0.85),
                               menu_label=self.label("Menu key", [75, 14], [12, 30], muted=True, scale=0.75),
                               menu=self.key(0, [84, 24]),
                               dark_label=self.label("Dark mode", [75, 14], [192, 30], muted=True, scale=0.75),
                               dark=self.toggle(17, [304, 27]))
        settings["bindings"] = [binding("#settings_open", "#visible")]
        dialog = node("panel", [WIDTH, "100% - 24px"], max_size=[WIDTH, 392],
                      cinnabar_transform={"scale": 1})
        dialog.update(anchor_from="center", anchor_to="center", bindings=[
            binding("#menu_offset", "#offset"), binding("#menu_alpha", "#alpha"),
            binding("#menu_scale", "#cinnabar_scale")],
            controls=children(
                background=self.rect(["100%", "100%"], radius=8, color=self.outer),
                brand=self.icon("brand", [15, 15], [15, 13]),
                title=self.label("Cinnaroids", [180, 17], [37, 9], scale=1.2),
                subtitle=self.label("Modules, ready when you are.", [240, 12], [37, 25], muted=True, scale=0.65),
                settings_button=self.button(28, [20, 22], [WIDTH - 57, 11], icon=self.icon("settings", [11, 11], [4, 5], muted=True)),
                close=self.button("mod.close", [20, 22], [WIDTH - 31, 11], icon=self.icon("close", [8, 8], [6, 6], muted=True)),
                modules=view, footer=footer, settings=settings))
        return dict(namespace="cinnaroids", panel=node("panel", ["100%", "100%"], controls=children(dialog=dialog)))

    def transitions(self):
        """Declare reversible panel motion and smooth accordion geometry on one retained route."""
        result = {
            "#menu_offset": dict(duration=0.25, easing=EASE, **{"from": [0, 6], "exit": [0, 6]}),
            "#menu_alpha": dict(duration=0.18, easing="out_cubic", **{"from": 0, "exit": 0}),
            "#menu_scale": dict(duration=0.25, easing=EASE, **{"from": 0.975, "exit": 0.975}),
            "#fold_content_height": dict(duration=0.25, easing=EASE),
        }
        for name, _, toggle, _, disclosure, _ in MODULES:
            result[f"#fold_{name}_offset"] = dict(duration=0.25, easing=EASE)
            if disclosure is not None:
                for prop in ["height", "angle"]:
                    result[f"#fold_{name}_{prop}"] = dict(duration=0.25, easing=EASE)
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
                        localize=False, text_alignment="left", font_scale_factor=0.85)
        if kind == "custom" and body.get("renderer") == "cinnabar_rounded_rectangle":
            colors = [key for key, value in bases.items() if key.startswith("base_rect_") and value["color"] == body["color"]]
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
    """Regenerate both deterministic native surfaces or verify their checked-in freshness."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    for dark in [True, False]:
        surface = Surface(dark)
        path = ROOT / "mods/cinnaroids/ui" / f"panel-{'dark' if dark else 'light'}.json"
        document = json.dumps(compact_document(surface.document()), separators=(",", ":"))
        text = json.dumps(dict(screen="cinnaroids.panel", document=document,
                               transitions=surface.transitions()), separators=(",", ":"))
        if args.check:
            if path.read_text() != text:
                raise SystemExit(f"{path.relative_to(ROOT)} needs regeneration")
        else:
            path.write_text(text)


if __name__ == "__main__":
    main()
