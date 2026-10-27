# In-game modules

Import `cinnaroids.wasm` through Cinnabar’s **Settings → Mods → Import**, approve its requested permissions and select **Enable**. [Installation and updates](install.md).

Press **F7** to open or close the module interface. Modules sit in Combat, Network and Render columns over the softened world; click a module to toggle it and use its arrow for settings. It manages:

- **Clicker:** hold the key or supported mouse button assigned to Attack to click at the selected CPS. The toggle key starts unbound and can be set in the panel.
- **Aim assist:** strength from 0–100%, with **Continuous** or **While clicking** activation. Enable **Horizontal only** to turn off vertical camera movement. Its toggle key starts unbound.
- **Reach:** adjustable local attack distance, bounded at six blocks. Its toggle key starts unbound.
- **FakeLag:** delay inbound and outbound game packets by 0–1000 milliseconds each. Default delay is 100 ms; the module starts off. Slider values can also be typed.
- **Netherite Finder:** highlight loaded ancient debris and netherite blocks with solid pink cubes through walls. Range is adjustable from 1–128 blocks (default 64), including typed values. It draws the nearest 256 matches and checks for changes four times a second; new and moved cubes appear over a few frames. Cubes stay in place while a menu or the panel is open. It cannot reveal terrain the server has not sent.
- **Player ESP:** trace observed players with a crisp body outline and a soft glow through walls. Range is 1–128 blocks (default 64), with Coral, Pink, Cyan or White outlines and independent Names, Distance and Health switches. Starts off; preferences persist.
- **Fullbright:** brighten world lighting without changing server data. Starts off; turning it off or unloading restores normal lighting.
- **Auto Jump Reset:** jump once on the first eligible ground contact after new server knockback. Its toggle key starts unbound. The hit window is 1–10 simulation ticks (default 4); it starts off.
- **Layout:** drag a column by its header to move it, or click the header to collapse it. Positions and collapsed columns are saved; **Reset layout** in the Settings column restores them.

Attack and jump checks follow your current keyboard and supported mouse bindings using the keys Cinnabar admits to gameplay. Keys consumed by shortcuts, held across a rebind, reserved, or blocked by menus, focus or the selected input device do not count. After a refused attack request, Clicker waits for the admitted attack hold to end before starting a new cadence. Clicker, While clicking aim assist and Auto Jump Reset pause while changed bindings refresh. Built-in touch and gamepad holds are unavailable through the current input API.

Slider values can also be typed: click the current number, enter a value, and press **Enter**. **Escape** cancels the edit. The host applies the slider's range and step. **Mode** opens a dropdown with explicit options instead of cycling when clicked. Click outside or press Escape to dismiss it. Editing consumes typing so module shortcuts do not toggle accidentally; the configured menu key retains its reserved action.

To change a module's binding, click its keycap, then press the next keyboard key. Each module has its own binding; a key already assigned to another module cannot be reused. Left-click is reserved for panel controls and is ignored during key capture. Previously saved left-click bindings clear when settings load. The configured **Menu key** is reserved for the panel. **Escape** clears a module binding during key capture and displays **None**; the cleared binding saves across restarts. For **Menu key**, Escape cancels capture and keeps the current binding. Outside key capture, Escape closes the panel. Closing the panel or leaving the game window cancels key capture.

Enabled switches are runtime state and start off for a new session. Disable the registration in Cinnabar’s Mods screen to unload the component; saved preferences are preserved. Opening the module interface releases gameplay input so its controls can be used normally.

Aim assist reads player coordinates, selects a target within six blocks and a 30-degree cone, and smoothly steers toward the chest. **Horizontal only** preserves your vertical aim while steering left or right; it defaults to off and saves with your preferences. Target selection still uses the same cone. Strength zero produces no motion. The API exposes positions without visibility or team information, so the module cannot filter occluded players or teammates. Menus, loss of cursor capture, and server-controlled cameras pause assistance.

Reach changes the client's actor attack selection distance. It preserves block targeting and normal server authority; a server can reject an attack outside its own range.

FakeLag delays application packets in both directions while preserving their order; the selected delay adds to each direction separately. Login and transport acknowledgments continue normally. Turning it off releases queued packets; disabling the component removes the delay. A host heartbeat expires the delay if the client stops responding. **Show real position** draws a translucent coral 3D box with thin edges at your own last movement position forwarded upstream, before newer movement leaves the delay queue. Display motion smoothly blends toward each sample; teleports and reconnects snap immediately. This is a sent-position witness, not a server acknowledgment; server corrections or rejected movement may differ. The box uses standing bounds, respects world depth, and hides while the camera is inside it, so it shows in third person. It clears when FakeLag or this option is off, on disconnect, or when its position feed becomes stale.

Auto Jump Reset observes server motion and requests an ordinary ground jump through Cinnabar's movement controls. It waits through descending airborne motion, expires missed landing windows, and leaves a held physical jump alone. Menus, focus loss, session changes and disabling the module discard pending assistance. A jump raises vertical motion; ground friction and opposing sprint input can reduce horizontal motion, but jumping does not generally erase horizontal knockback or guarantee vertical-only results. Server motion events also include explosions and launches, so the host cannot identify every event as a player attack. This module requires a Cinnabar build with the movement capability; older hosts cannot load the updated component.

Player ESP outlines the host's actual animated player and equipment meshes, with a matching soft halo. Native bounds place the labels, which Cinnaroids draws on its own HUD layer and Cinnabar keeps above each player as it moves; no box geometry is drawn. A player keeps the same label while in view, and up to three new names appear per frame. It draws at most six observed players per callback and cannot reveal actors the server has not sent. The host's observation budget also includes non-player actors, so crowded scenes may expose fewer players. Missing bounds are skipped. Health displays rounded-up current/maximum HP in the label; when maximum health is unknown, only reported HP is shown. Missing health is omitted entirely. Names and distance can be hidden independently. Opening the panel, losing focus, disconnecting or disabling ESP clears the overlay and retained labels.

Terrain and ESP updates spread across additional frames when other enabled modules use the callback budget. Combat input checks and aim assistance continue on every frame. Under the maximum supported desktop workload, stable ESP observations converge within 48 callbacks, and stable terrain within 192 callbacks, including an earlier queued answer. Disabling Finder clears its cubes within 32 callbacks; ESP and the position marker clear within one callback. Panel updates and saved preferences converge within 12 callbacks after input settles. A complete 128-row binding snapshot takes 32 callbacks.

The callback budget tests cover maximum input counts together, escaped 128-byte actor names, clustered terrain positions, continuous name changes and drawing-module toggles. The current desktop key vocabulary uses the fixed-cost resolver. Arbitrary future key names retain an exact compatibility fallback; combining many such names with every module can exceed the 100,000-fuel target. That vocabulary requires a new budget pass before an input adapter emits it.

Player ESP requires a Cinnabar build containing actor mask outline/glow composition and the updated actor observation record. Entity highlighting, protected observation and through-wall permissions must be granted in Mods, and Cinnabar's session policy must allow protected observations and through-wall rendering. Current Cinnabar requires an authenticated server grant for elevated drawing on remote servers; local-world policy is handled by the host. ESP does not bypass that policy.

## Runtime and settings

Cinnabar owns the registration, permissions and component lifecycle. Preferences save to the component's fixed companion settings file. Runtime enabled switches start off whenever the component initializes. The WASM has no WASI or ambient filesystem, network or operating-system input access.

Requested permissions cover the HUD, player snapshots, camera, controls, interaction, movement, settings, packet delay, terrain, Fullbright, world drawing, entity highlighting, protected entity observations and through-wall rendering. Netherite Finder needs terrain and world drawing, and through-wall rendering to show cubes behind blocks; without it the cubes respect world depth. Show real position needs packet delay and world drawing. Importing the file does not grant them automatically; review them in Mods. Features using a denied permission cannot operate.

The panel uses Cinnabar's installed font. Panel sizing follows display DPI independently of Minecraft's GUI scale. Change **Menu key** in the panel by clicking the keycap and pressing a key; the binding saves across restarts. F7 is the default.

## Build

```sh
python3 scripts/build-cinnaroids.py
cargo test --locked -p cinnaroids-mod
```

Rust 1.99.0, `wasm32-unknown-unknown`, and Python 3.11 or newer are required. The result is `dist/cinnaroids.wasm`; there is no native executable. On Windows, `scripts/build-cinnaroids.ps1` invokes the same build.

To check actual WASM execution against a matching Cinnabar source checkout, run `scripts/verify-host-budget.ps1 -HostRepo PATH` after building. It exercises startup with saved preferences and panel changes without starting a game.
