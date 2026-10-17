---
name: Cinnaroids ClickGUI
description: Floating category columns over a softened world, lit by one coral accent.
colors:
  accent: "#ff8a65"
  accent-tint: "rgba(255, 138, 101, 0.17)"
  capture-fill: "#2a1d19"
  column-fill: "rgba(15, 17, 21, 0.9)"
  detail-fill: "rgba(0, 0, 0, 0.24)"
  text: "#e6e8eb"
  muted: "#a3a9b2"
  knob: "#f4f5f7"
  hover: "rgba(255, 255, 255, 0.05)"
  field: "rgba(255, 255, 255, 0.06)"
  divider: "rgba(255, 255, 255, 0.09)"
  edge: "rgba(255, 255, 255, 0.13)"
  drag-edge: "rgba(255, 255, 255, 0.24)"
typography:
  column-title: {fontFamily: "Inter Medium", fontWeight: 500, fontSize: "1.06em"}
  module: {fontFamily: "Inter Medium", fontWeight: 500, fontSize: "0.92em"}
  strong: {fontFamily: "Inter Medium", fontWeight: 500, fontSize: "0.86em"}
  label: {fontFamily: "Inter Medium", fontWeight: 500, fontSize: "0.78em"}
  caption: {fontFamily: "Inter Medium", fontWeight: 500, fontSize: "0.75em"}
  small: {fontFamily: "Inter Medium", fontWeight: 500, fontSize: "0.72em"}
  legend: {fontFamily: "Inter Medium", fontWeight: 500, fontSize: "0.66em"}
rounded:
  plate: "5px"
  field: "3px"
  track: "1px"
  pill: "5px"
spacing:
  inset: "9px"
  text-left: "10px"
  row: "20px"
  header: "22px"
  footer: "5px"
  column: "150px"
  column-gap: "53px"
components:
  column: {width: "{spacing.column}", backgroundColor: "{colors.column-fill}", rounded: "{rounded.plate}"}
  column-header: {height: "{spacing.header}", textColor: "{colors.text}", typography: "{typography.column-title}"}
  module-row: {height: "{spacing.row}", textColor: "{colors.text}", typography: "{typography.module}"}
  module-row-enabled: {height: "{spacing.row}", backgroundColor: "{colors.accent-tint}"}
  settings-region: {width: "{spacing.column}", backgroundColor: "{colors.detail-fill}"}
  field: {height: "16px", backgroundColor: "{colors.field}", rounded: "{rounded.field}", typography: "{typography.label}"}
  keycap: {height: "16px", width: "58px", backgroundColor: "{colors.field}", textColor: "{colors.text}", rounded: "{rounded.field}", typography: "{typography.caption}"}
  keycap-capture: {height: "16px", backgroundColor: "{colors.capture-fill}", textColor: "{colors.accent}", rounded: "{rounded.field}", typography: "{typography.small}"}
  row-keycap: {height: "12px", width: "20px", backgroundColor: "{colors.field}", rounded: "{rounded.field}", typography: "{typography.legend}"}
  switch: {height: "10px", width: "18px", backgroundColor: "{colors.edge}", rounded: "{rounded.pill}"}
  switch-on: {backgroundColor: "{colors.accent}"}
  slider: {height: "2px", backgroundColor: "{colors.edge}", rounded: "{rounded.track}"}
  slider-fill: {height: "2px", backgroundColor: "{colors.accent}", rounded: "{rounded.track}"}
---
# Design System: Cinnaroids ClickGUI

## Overview

**Creative North Star: "The Lit Column"**

The classic client-mod ClickGUI played straight. Category columns (Combat, Network, Render, and Settings) float over a world that softens behind them: blur 4, brightness 0.7, saturation 0.85. Each column is one near-black translucent plate; enabled modules light up with a coral tint and a 2-unit coral bar on the left edge. Settings open in a darker region directly beneath their row. There is no modal card and no light theme.

This is a native Cinnabar overlay, not a web page. Every dimension below is in logical GUI units (rendered evidence is 2 physical px per unit). The host scales the 760x420 reference box down on small viewports, so the box stays visible.

**Key Characteristics:**
- One accent, coral, used only for state: enabled, filled, and capturing.
- Flat translucent plates, no shadows. Depth comes from alpha layers.
- Dense 20-unit rows; values and key binds readable without opening anything.
- Motion on bound scalars only: an expo settle in, a quick reversible exit, and drags that follow the pointer exactly.

## Colors

A cool near-black and grey neutral set with a single warm accent.

### Primary
- **Coral** (accent): enabled-row lift bar, switch-on track, slider fill, bound key legend on enabled rows, the capture border and prompt. Nothing decorative.
- **Coral Glow** (accent-tint): the enabled-row wash. It fades in with toggle progress, so it shows as a warm brown over the column.
- **Ember** (capture-fill): the opaque fill under a keycap while it waits for a key, with a 1-unit coral border.

### Neutral
- **Night Plate** (column-fill): column bodies.
- **Well** (detail-fill): black at 24% behind expanded settings, one step darker than the column.
- **Bone** (text): names, values in fields, and keycap legends.
- **Ash** (muted): setting labels, trailing row values, units, column counts, and chevrons.
- **Knob** (knob): slider and switch knobs.
- **White alphas**: hover feedback (5%), field fills (6%), header divider (9%), track and edge (13%), drag lift border (24%). The header shows a rounded 4% tint on hover and 7% while pressed.

### Named Rules
**The State-Only Coral Rule.** Coral means on, filled, or listening. If an element does not report state, it uses no coral.

## Typography

**Font:** Inter Medium (500), embedded as the native `mod_panel` face; one weight. Labels never draw a shadow. Keep the Inter OFL notice (`assets/fonts/Inter-OFL-1.1.txt`) shipped with the component.

**Character:** One neutral grotesque. Hierarchy comes from scale and the text/muted split, not from weight.

### Hierarchy
Sizes are Cinnabar `font_scale_factor` values, written as `em` in the frontmatter. They are renderer factors, not CSS sizes.
- **Column title** (1.06): column headers.
- **Module** (0.92): module names in rows.
- **Strong** (0.86): the "Menu key" label. "Reset layout" uses 0.82.
- **Label** (0.78): setting titles, trailing values, slider values, dropdown values, and column counts.
- **Caption** (0.75): keycap legends in settings.
- **Small** (0.72): units and the "Press a key" prompt.
- **Legend** (0.66): the small keycap inside a module row.

### Named Rules
**The Font Range Rule.** Copy must stay within the embedded glyph set: ASCII plus `· × — • ‹ › −`. Separators use the middle dot (`·`). Icons are Cinnabar vector icons (chevron, brand), never text glyphs.

## Layout

The desk is centred on a 760x420 reference box. Columns are 150 wide. By default they share one top line, 44 units below the box top, with origins at x = -380, -177, 27, and 230 from centre, leaving 53–54 unit gaps. Module columns drag anywhere on screen; the grabbed header stays under the pointer. The Settings column, which holds Reset layout, stays inside the box.

A column is a 22 header, its stacked rows, and a 5-unit footer under the last row. Rows are 20 high. Text starts 10 from the left edge and indents to 13 when a row is enabled. Expandable rows give their right 22 units to the disclosure arrow. Settings regions use a 9 inset and stack from y 6: slider 30, dropdown 32, toggle 20, and Bind 22, plus 2 at the bottom. That gives region heights of 60 (single slider with Bind), 92 (Aim assist), 58 (FakeLag), and 38 (Netherite Finder). Fullbright has no region.

Rows can expand independently, and several can be open at once. Clicking a header without moving it collapses or expands that column. Moving 3 units turns the press into a drag. Column positions and collapsed flags persist. Reset layout restores both. Enabled flags and expansion are runtime only.

## Elevation & Depth

Flat, with no shadows anywhere. Depth comes from the softened world behind the panel and three alpha layers: the column plate, the darker settings well, and white-alpha fields. Only a drag lifts a column, by fading its 0.5-unit border to drag-edge.

## Shapes

Column plates use a 5 radius. Fields, keycaps, and the dropdown use 3. Slider tracks are 2-unit lines with a 1 radius. Switches (18x10) and knobs (6x6) are fully rounded. Rows, tints, the lift bar, and the settings well are square-cornered inside the plate. The 5-unit footer, equal to the plate radius, keeps them clear of the bottom corners. Header hover and press tints are rounded header shapes; the header clip squares only the seam with the body. Inside a header, a 0.5-unit hairline divider runs from inset to inset.

## Components

### Column
A 150-wide Night Plate. The header holds the title at x 10, an Ash count ("2/4") right-aligned to x 130, and a chevron at x 136. The chevron points down when open and rotates -90 when collapsed. The whole header is a drag grip.

### Module row
Left-click toggles the row. On toggle, Coral Glow fades in, the 2x20 coral bar scales up from its centre with slight overshoot, and the name moves from x 10 to x 13. The trailing value is always Ash. When a toggle key is bound, a 20x12 row keycap fades in. Its legend is coral on enabled rows and Bone on disabled ones, and the value slides left to make room. The separate 22-wide arrow opens settings: it rotates from -90 to 0 while the region's height, the rows below it, and a content fade animate together.

### Settings controls
- **Slider:** an Ash title on the left and a value plus unit on the right; the value is a native typed editor. The track spans the full inset width. The coral fill carries a 6x6 knob.
- **Dropdown:** a 16-high field with the value at x 6 and a chevron at the right. Selection uses the host dropdown.
- **Switch:** an 18x10 track whose coral layer fades with toggle progress. The knob slides natively.
- **Bind:** a 58x16 keycap with an edge border. During capture, an Ember cap with a coral border and a "Press a key" prompt fades in over it. Esc clears a module key and keeps the menu key.

### Settings column
The Settings column is visible alongside Combat, Network, and Render. It shares their header, drag grip, collapse behavior, and entrance motion, and contains Menu key and Reset layout.

### Motion
Transitions run on bound scalars, with no delays or loops.
- **Open:** the world eases in over 380ms in-out-cubic. Columns rise 10 units over 300, 345, 390, and 435ms expo, staggered by 45ms, and fade over 140ms plus 20ms per column.
- **Close:** the same curves reverse from shorter distances (3 and 5). The world's symmetric curve keeps it dimmed until the columns have faded out.
- **State:** height, row offset, and plate height take 260ms expo. Chevrons and the name indent take 220ms and 200ms settle (0.2, 0.9, 0.25, 1). The lift bar takes 240ms with overshoot (0.34, 1.36, 0.64, 1). Settings content fades over 220ms out-cubic. The divider and keycap fade over 140–180ms out-cubic.
- **Drag:** no easing, so the column tracks the pointer exactly.

### Delivery
`scripts/generate-panel.py` is the single source of geometry, color, and transitions, emitted to `mods/cinnaroids/ui/panel.json` and embedded in the component. `panel.rs` publishes only live scalar bindings and the control shell. The shell is hand-assembled from static fragments to stay within the 100k per-frame guest fuel budget, so keep it that way.

## Do's and Don'ts

### Do:
- **Do** keep every value in `generate-panel.py` and drive changes through bound scalars; let the host fit the 760x420 box.
- **Do** keep toggling, disclosure, and key binding as three separate actions on separate hit areas.
- **Do** show each module's value and bound key in its row without requiring it to open.
- **Do** use coral only for state (the State-Only Coral Rule).
- **Do** build the control shell from static fragments in `panel.rs`.

### Don't:
- **Don't** add shadows, gradients, or a second accent; depth is alpha layering on flat plates.
- **Don't** fold the columns back into a single modal dialog card or reintroduce a light theme.
- **Don't** round rows, tints, or the settings well; only plates, fields, switches, and knobs carry radii.
- **Don't** use characters outside Inter Medium's embedded set or draw icons as text.
- **Don't** persist enabled flags or row expansion.
- **Don't** ease column drags.
