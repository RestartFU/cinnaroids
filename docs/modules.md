# In-game modules

Import `cinnaroids.wasm` through Cinnabar’s **Settings → Mods → Import**, approve its requested permissions and select **Enable**. [Installation and updates](install.md).

Press **F7** to open or close the module interface. All modules and settings share one panel without category tabs. It manages:

- **Clicker:** hold the physical Attack control to click at the selected CPS. The toggle key is configurable; F8 is the default.
- **Aim assist:** strength from 0–100%, with **Continuous** or **While clicking** activation. Its default toggle key is R.
- **Reach:** adjustable local attack distance, bounded at six blocks. Its default toggle key is V.
- **FakeLag:** delay inbound and outbound game packets by 0–1000 milliseconds each. Default delay is 100 ms; the module starts off. Slider values can also be typed.
- **Netherite Finder:** highlight loaded ancient debris and netherite blocks with solid pink cubes through walls. Range is adjustable from 1–128 blocks (default 64), including typed values. It cannot reveal terrain the server has not sent.
- **Fullbright:** brighten world lighting without changing server data. Starts off; F10 or unloading restores normal lighting.
- **Auto Jump Reset:** jump once on the first eligible ground contact after new server knockback. Default toggle key is B. The hit window is 1–10 simulation ticks (default 4); it starts off.
- **Appearance:** neutral dark mode by default, with a light option.

Slider values can also be typed: click the current number, enter a value, and press **Enter**. **Escape** cancels the edit. The host applies the slider's range and step. **Mode** opens a dropdown with explicit options instead of cycling when clicked. Click outside or press Escape to dismiss it. Editing consumes typing so module shortcuts do not toggle accidentally; **F10** and the configured menu key retain their reserved actions.

To change a module's binding, press its **Keybind** keycap, then press the next keyboard key. Each module has its own binding; a key already assigned to another module cannot be reused. Left-click is reserved for panel controls and is ignored during key capture. Previously saved left-click bindings reset to their defaults when settings load. The configured **Menu key** and **F10** are reserved for the panel and emergency stop. **Escape** can be assigned during key capture; otherwise it closes the panel. Closing the panel or leaving the game window cancels key capture.

**F10** stops all modules. Enabled switches are runtime state and start off for a new session. Disable the registration in Cinnabar’s Mods screen to unload the component; saved preferences are preserved. Opening the module interface releases gameplay input so its controls can be used normally.

Aim assist reads player coordinates, selects a target within six blocks and a 30-degree cone, and smoothly steers toward the chest. Strength zero produces no motion. The API exposes positions without visibility or team information, so the module cannot filter occluded players or teammates. Menus, loss of cursor capture, and server-controlled cameras pause assistance.

Reach changes the client's actor attack selection distance. It preserves block targeting and normal server authority; a server can reject an attack outside its own range.

FakeLag delays application packets in both directions while preserving their order; the selected delay adds to each direction separately. Login and transport acknowledgments continue normally. Turning it off or pressing F10 releases queued packets; disabling the component removes the delay. A host heartbeat expires the delay if the client stops responding. **Show real position** draws a translucent coral 3D box with antialiased edges at your own last movement position forwarded upstream, before newer movement leaves the delay queue. Display motion smoothly blends toward each sample; teleports and reconnects snap immediately. This is a sent-position witness, not a server acknowledgment; server corrections or rejected movement may differ. The box uses standing bounds, respects world depth, and hides when the first-person camera is inside it. It clears when FakeLag or this option is off, on disconnect, or when its position feed becomes stale.

Auto Jump Reset observes server motion and requests an ordinary ground jump through Cinnabar's movement controls. It waits through descending airborne motion, expires missed landing windows, and leaves a held physical jump alone. Menus, focus loss, session changes and F10 discard pending assistance. A jump raises vertical motion; ground friction and opposing sprint input can reduce horizontal motion, but jumping does not generally erase horizontal knockback or guarantee vertical-only results. Server motion events also include explosions and launches, so the host cannot identify every event as a player attack. This module requires a Cinnabar build with the movement capability; older hosts cannot load the updated component.

## Runtime and settings

Cinnabar owns the registration, permissions and component lifecycle. Preferences save to the component's fixed companion settings file. Runtime enabled switches start off whenever the component initializes. The WASM has no WASI or ambient filesystem, network or operating-system input access.

Requested permissions cover the HUD, player snapshots, camera, controls, interaction, movement, settings, packet delay, block highlights and Fullbright. Importing the file does not grant them automatically; review them in Mods. Features using a denied permission cannot operate.

The panel uses Cinnabar's installed font. Panel sizing follows display DPI independently of Minecraft's GUI scale. Change **Menu key** in the panel by clicking the keycap and pressing a key; the binding saves across restarts. F7 is the default, and F10 remains the emergency stop.

## Build

```sh
python3 scripts/build-cinnaroids.py
cargo test --locked -p cinnaroids-mod
```

Rust 1.99.0, `wasm32-unknown-unknown`, and Python 3.11 or newer are required. The result is `dist/cinnaroids.wasm`; there is no native executable. On Windows, `scripts/build-cinnaroids.ps1` invokes the same build.

To check actual WASM execution against a matching Cinnabar source checkout, run `scripts/verify-host-budget.ps1 -HostRepo PATH` after building. It exercises startup with saved preferences and panel changes without starting a game.
