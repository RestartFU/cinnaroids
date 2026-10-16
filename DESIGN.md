---
name: Cinnaroids Fold
description: Compact native module disclosures in aubergine and coral.
colors:
  dark-surface: "#201b23"
  dark-inset: "#302630"
  dark-hover: "#3b303b"
  dark-divider: "#4d3e4b"
  dark-text: "#f1ece9"
  dark-muted: "#c0b3bb"
  dark-accent: "#ff9a85"
  dark-track: "#8c7487"
  light-surface: "#f3eded"
  light-inset: "#e9dfe3"
  light-hover: "#dfd1d8"
  light-divider: "#cbb7c2"
  light-text: "#3d2838"
  light-muted: "#76576b"
  light-accent: "#a84239"
  light-track: "#ad8e9f"
typography:
  body: {fontFamily: "Inter Medium", fontWeight: 500}
rounded:
  panel: "8px"
  field: "3px"
  switch: "5px"
spacing:
  row: "32px"
  chrome: "44px"
  content-gutter: "28px"
components:
  module-row: {height: "{spacing.row}"}
  panel: {width: "370px", backgroundColor: "{colors.dark-surface}", rounded: "{rounded.panel}"}
---
# Fold menu

## Overview

One native, scrollable module list in the approved aubergine and coral palette. Module names disclose settings independently of switches and key bindings. Fullbright stays one compact row. Header and emergency controls remain visible while the list scrolls.

## Colors

Coral identifies enabled switches, slider fill and the brand mark. Aubergine layers distinguish the menu, expanded settings and hover feedback. Pale plum and dark coral preserve that hierarchy in light mode. Frontmatter contains the palette values; the generator converts them to native RGBA.

## Typography

The component embeds Inter Medium under the native `mod_panel` face, without shadows. Native font scale factors are 1.2 for the title, .9 for module names, .85 for standard labels, .8 for dropdowns, .75 for settings labels, .7 for summaries/keys and .65 for supporting copy. These are renderer factors, not CSS pixel sizes. Preserve the Inter OFL notice.

## Layout

Dimensions in this document are native logical GUI pixels. The dialog is 370 wide and at most 392 high; short windows reduce its height and scroll the list. Rows are 32 high; header/footer 44 each. Setting regions fit their controls:48 high for a single slider,80 for Aim assist and 68 for FakeLag. Native reference fitting handles smaller viewports.

At most one module is expanded; selecting it again collapses it. Aim assist is expanded when module state initializes. Preferences, including appearance, persist; disclosure, settings-overlay visibility and enabled flags are transient.

## Elevation & Depth

Opaque surfaces and inset tones establish depth. Text stays sharp throughout the transition. Scale, offset and opacity establish focus.

## Shapes

The dialog uses an 8px radius; fields and keycaps 3px, switches 5px. Compact separators align the rows. Fullbright has no expanded region.

## Components

Opening and closing ease offset 6px and scale.975 over 250ms with cubic-bezier(.16,1,.3,1); opacity uses 180ms out-cubic. Disclosures ease clipped height, following rows and chevrons over 250ms on the retained route. Cinnabar supplies 100ms hover/toggle feedback and 140ms discrete wheel easing. Screen animations settles all motion immediately; closing releases input before the exit finishes.

`generate-panel.py` owns both native documents and their static templates. Packaging embeds the immutable theme routes in the WASM `PANEL_BUNDLE_SECTION`. When panel state is dirty, the callback sends the small control shell and route/binding patch together through `set_content` and `update_content`. Full documents come from `panel_json()` for packaging and previews. `panel.rs` supplies bounded live bindings; `state.rs` owns navigation and control state.

Summaries and enabled count reflect current state. Numeric editors, dropdown, shortcuts, Escape clearing and F 10 stay native. Existing modules without shortcut preferences do not gain decorative keycaps.

## Do's and Don'ts

- Do keep disclosure separate from enablement and preserve the original palette.
- Do clip collapsed controls and keep emergency actions visible.
- Don't add empty card space around Fullbright.
- Don't store navigation or enabled flags in preferences.
