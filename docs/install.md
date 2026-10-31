# Install Cinnaroids

1. Update Cinnabar to a build with [movement input observation and overrides](https://github.com/cinnabar-mc/cinnabar/pull/2519) and **Settings → Mods → Import**.
2. Download `cinnaroids.wasm` from the [latest release](https://github.com/RestartFU/cinnaroids/releases/latest). The same file works on all Cinnabar desktop platforms.
3. In Cinnabar, open **Settings → Mods → Import** and select the WASM file.
4. Review and approve its requested permissions, then select **Enable**.
5. Press **F7** to open the Cinnaroids panel.

The release includes `cinnaroids.wasm.sha256` for verification. For example, put both downloads in the same directory and run `sha256sum --check cinnaroids.wasm.sha256` on Linux or `shasum -a 256 --check cinnaroids.wasm.sha256` on macOS. Copyright and license notices are embedded in the WASM.

To update while keeping the same registration and preferences, replace the installed component at its current path with the new `cinnaroids.wasm`, then select **Reload** in Mods. Use a temporary sibling file and rename it over the old file to avoid loading a partially copied update. Importing a differently named file creates another registration.

Updates preserve existing permission approvals. For AutoTotem, open the Cinnaroids permissions and approve **Inventory requests** and **Player status**, then reload the mod. Reloading alone does not approve new permissions. Modules with missing required permissions stay off; their dimmed rows explain which permissions are needed. Shortcuts also respect those permissions.

Use **Disable** in Mods to unload Cinnaroids. Enabled registrations persist across client restarts; individual gameplay module switches also persist and restore when their required permissions are granted. Settings belong to Cinnabar's companion settings file beside the installed component.

If migrating from the old launcher, close it and disable its old registration before importing the WASM. The launcher and its shell installer are no longer distributed. Old launcher paths and automatic preference migration are not used by the Mods import flow; keep a copy of your old component settings if you want to migrate them manually.
