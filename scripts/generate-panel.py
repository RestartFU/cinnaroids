#!/usr/bin/env python3
"""Generate the tab-free card surface used by Cinnabar's native panel renderer."""

import argparse
import json
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
CARD = 132
GAP = 8


def node(kind, size, offset=(0, 0), **properties):
    """Place a JSON-UI node relative to its parent's upper-left corner."""
    return dict(type=kind, size=list(size), offset=list(offset),
                anchor_from="top_left", anchor_to="top_left", **properties)


def children(**nodes):
    """Give each child a stable name for native hit testing and scrolling."""
    return [{name: body} for name, body in nodes.items()]


def binding(name, target):
    """Read an existing host-owned control binding into one visual property."""
    return dict(binding_name=name, binding_name_override=target)


class Surface:
    """Author square cards using the existing compact panel's native widgets."""

    def __init__(self, dark):
        self.card_color = [0.075, 0.09, 0.115, 0.96] if dark else [0.96, 0.965, 0.975, 0.97]
        self.outer = [0.10, 0.13, 0.18, 0.95] if dark else [0.92, 0.93, 0.95, 0.98]
        self.border = [0.22, 0.26, 0.31, 0.9] if dark else [0.75, 0.77, 0.8, 0.95]
        self.raised = [0.16, 0.185, 0.22, 0.98] if dark else [0.86, 0.875, 0.9, 1]
        self.text = [0.94, 0.95, 0.97, 1] if dark else [0.11, 0.12, 0.14, 1]
        self.muted = [0.67, 0.71, 0.76, 1] if dark else [0.39, 0.41, 0.45, 1]
        self.accent = [1, 0.34, 0.32, 1] if dark else [0.78, 0.24, 0.20, 1]

    def rect(self, size, offset=(0, 0), radius=0, color=None):
        """Use the host's rounded rectangle painter, with no texture dependency."""
        return node("custom", size, offset, renderer="cinnabar_rounded_rectangle",
                    radius=radius, color=color if color is not None else self.border)

    def label(self, text, size, offset=(0, 0), *, muted=False, scale=0.85, align="left"):
        """Draw a literal label or a changing host-formatted control value."""
        result = node("label", size, offset, text=text, text_alignment=align,
                      color=self.muted if muted else self.text, shadow=False,
                      clips_children=True, hide_hyphen=True, localize=False,
                      font_type="mod_panel", font_scale_factor=scale)
        if text.startswith("#"):
            result["bindings"] = [dict(binding_name=text)]
        return result

    def icon(self, name, size, offset):
        """Draw an icon through Cinnabar's installed vector icon painter."""
        return node("custom", size, offset, renderer="cinnabar_vector_icon",
                    icon=name, color=self.accent if name != "close" else self.muted)

    def button(self, action, size, offset=(0, 0), **contents):
        """Route a pressed native button to an existing declared control."""
        return node("button", size, offset, controls=children(**contents),
                    button_mappings=[dict(from_button_id="button.menu_select",
                                          to_button_id=action, mapping_type="pressed")])

    def toggle(self, index, offset):
        """Bind the switch track and knob to the host's live toggle state."""
        track = self.rect([20, 10], radius=5, color=self.raised)
        track["bindings"] = [binding(f"#row_{index}_toggle_color", "#color")]
        knob = self.rect([6, 6], [2, 2], 3, self.text)
        knob["bindings"] = [binding(f"#row_{index}_knob_offset", "#offset"),
                            binding(f"#row_{index}_knob_color", "#color")]
        return self.button(f"mod.control:{index}", [20, 14], offset, track=track, knob=knob)

    def slider(self, index, text, y):
        """Keep continuous dragging and click-to-type values on the native routes."""
        width = CARD - 20
        fill = self.rect([width, 2], [0, 3.5], 1, self.accent)
        fill["bindings"] = [binding(f"#row_{index}_fill", "#size_binding_x")]
        knob = self.rect([5, 5], radius=2.5, color=self.text)
        knob.update(anchor_from="right_middle", anchor_to="center")
        fill["controls"] = children(knob=knob)
        return node("panel", [width, 32], [10, y], controls=children(
            label=self.label(text, [width - 30, 12], muted=True),
            value=self.button(f"mod.edit:{index}", [30, 13], [width - 30, 0],
                              text=self.label(f"#row_{index}_value", [30, 12], align="right")),
            slider=self.button(f"mod.control:{index}", [width, 12], [0, 11],
                               track=self.rect([width, 2], [0, 3.5], 1, self.raised), fill=fill)))

    def keybind(self, index, y, text="Keybind"):
        """Show the native capture indicator and the persisted binding or None."""
        return node("panel", [CARD - 20, 28], [10, y], controls=children(
            label=self.label(text, [CARD - 58, 16], [0, 7], muted=True),
            key=self.button(f"mod.control:{index}", [32, 22], [CARD - 52, 0],
                            background=self.rect([32, 22], radius=5, color=self.raised),
                            value=self.label(f"#row_{index}_value", [32, 15], [0, 7],
                                             scale=0.75, align="center"))))

    def choice(self, index, y):
        """Open the host's keyboard-accessible dropdown instead of cycling options."""
        return node("panel", [CARD - 20, 26], [10, y], controls=children(
            label=self.label("Mode", [38, 14], [0, 7], muted=True),
            choice=self.button(f"mod.control:{index}", [74, 22], [38, 0],
                               background=self.rect([74, 22], radius=4, color=self.raised),
                               value=self.label(f"#row_{index}_value", [60, 14], [4, 7], scale=0.75),
                               chevron=self.icon("chevron", [5, 3], [64, 10]))))

    def card(self, name, label, column, row, toggle=None, icon=None, **contents):
        """Keep every module in its own square within one continuous card grid."""
        title_x = 30 if icon else 10
        body = dict(border=self.rect([CARD, CARD], radius=8),
                    background=self.rect([CARD - 1, CARD - 1], [0.5, 0.5], 7.5, self.card_color),
                    title=self.label(label, [CARD - title_x - (32 if toggle is not None else 10), 17],
                                     [title_x, 13], scale=1 if len(label) < 14 else 0.85),
                    rule=self.rect([CARD - 20, 0.5], [10, 31]))
        if icon:
            body["icon"] = self.icon(icon, [13, 13], [10, 11])
        if toggle is not None:
            body["toggle"] = self.toggle(toggle, [CARD - 30, 13])
        body.update(contents)
        return {name: node("panel", [CARD, CARD],
                           [column * (CARD + GAP), row * (CARD + GAP)], controls=children(**body))}

    def document(self):
        """Keep the header fixed while all eight cards scroll as one native surface."""
        cards = [
            self.card("clicker", "Clicker", 0, 0, 1, "pointer",
                      cps=self.slider(2, "CPS", 40), key=self.keybind(3, 101)),
            self.card("aim", "Aim assist", 1, 0, 4, "crosshair",
                      strength=self.slider(5, "Strength", 40), mode=self.choice(6, 72), key=self.keybind(7, 101)),
            self.card("reach", "Reach", 2, 0, 8, "ruler",
                      distance=self.slider(9, "Distance", 40), key=self.keybind(10, 101)),
            self.card("fake_lag", "FakeLag", 0, 1, 14,
                      delay=self.slider(15, "Delay (ms)", 40),
                      real_label=self.label("Show real position", [90, 15], [10, 98], scale=0.75),
                      real_toggle=self.toggle(16, [CARD - 30, 98])),
            self.card("jump_reset", "Auto Jump Reset", 1, 1, 11,
                      window=self.slider(12, "Hit window (ticks)", 40), key=self.keybind(13, 101)),
            self.card("netherite", "Netherite Finder", 2, 1, 18,
                      range=self.slider(19, "Range", 40)),
            self.card("fullbright", "Fullbright", 0, 2, 20),
            self.card("general", "General", 1, 2, icon="settings",
                      menu=self.keybind(0, 40, "Menu key"),
                      dark_label=self.label("Dark mode", [82, 15], [10, 77]),
                      dark_toggle=self.toggle(17, [CARD - 30, 77]),
                      stop=self.button("mod.control:21", [CARD - 20, 21], [10, 101],
                                       background=self.rect([CARD - 20, 21], radius=4, color=self.raised),
                                       label=self.label("Disable all", [CARD - 20, 15], [0, 6], align="center"))),
        ]
        track = node("scroll_track", [4, "100%"], controls=children(
            background=self.rect(["100%", "100%"], radius=2, color=self.raised),
            thumb=node("scrollbar_box", ["100%", "100%"], draggable="vertical", controls=children(
                background=self.rect(["100%", "100%"], radius=2, color=self.muted)))))
        view = node("scroll_view", [420, "100% - 56px"], [10, 46],
                    scroll_content="cards", scroll_view_port="viewport", scrollbar_track="track",
                    scrollbar_box="thumb", scroll_box_and_track_panel="scrollbar", scroll_speed=24,
                    controls=children(
                        viewport=node("panel", [412, "100%"], clips_children=True, controls=children(
                            cards=node("panel", [412, 3 * CARD + 2 * GAP], controls=cards))),
                        scrollbar=node("panel", [4, "100%"], [416, 0], controls=children(track=track))))
        dialog = node("panel", [440, "100% - 40px"], [0, 20], max_size=[440, 468])
        dialog.update(anchor_from="top_middle", anchor_to="top_middle", controls=children(
            border=self.rect(["100%", "100%"], radius=12),
            background=self.rect(["100% - 1px", "100% - 1px"], [0.5, 0.5], 11.5, self.outer),
            brand=self.icon("brand", [18, 18], [12, 9]),
            title=self.label("Cinnaroids", [160, 20], [39, 12], scale=1),
            close=self.button("mod.close", [20, 24], [413, 6], icon=self.icon("close", [9, 9], [5, 7])),
            rule=self.rect([438, 0.5], [1, 35.5]),
            modules=view))
        return dict(namespace="cinnaroids", panel=node("panel", ["100%", "100%"], controls=children(dialog=dialog)))


def main():
    """Write deterministic assets, or check that committed assets match the generator."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    for dark in [True, False]:
        path = ROOT / "mods/cinnaroids/ui" / f"panel-{'dark' if dark else 'light'}.json"
        document = json.dumps(Surface(dark).document(), separators=(",", ":"))
        text = json.dumps(dict(screen="cinnaroids.panel", document=document), separators=(",", ":"))
        if args.check:
            if path.read_text() != text:
                raise SystemExit(f"{path.relative_to(ROOT)} needs regeneration")
        else:
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(text)


if __name__ == "__main__":
    main()
