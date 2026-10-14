# Install Cinnaroids

1. Update Cinnabar to a build with **Settings → Mods → Import**.
2. Download `cinnaroids.wasm` from the [latest release](https://github.com/RestartFU/cinnaroids/releases/latest). The same file works on all Cinnabar desktop platforms.
3. In Cinnabar, open **Settings → Mods → Import** and select the WASM file.
4. Review and approve its requested permissions, then select **Enable**.
5. Press **F7** to open the Cinnaroids panel. **F10** disables all modules.

The release includes `cinnaroids.wasm.sha256` for verification. For example, put both downloads in the same directory and run `sha256sum --check cinnaroids.wasm.sha256` on Linux or `shasum -a 256 --check cinnaroids.wasm.sha256` on macOS. Copyright and license notices are embedded in the WASM.

To update while keeping the same registration and preferences, replace the installed component at its current path with the new `cinnaroids.wasm`, then select **Reload** in Mods. Use a temporary sibling file and rename it over the old file to avoid loading a partially copied update. Importing a differently named file creates another registration.

Use **Disable** in Mods to unload Cinnaroids. Enabled registrations persist across client restarts; the individual gameplay modules start disabled when the component initializes. Settings belong to Cinnabar's companion settings file beside the installed component.

If migrating from the old launcher, close it and disable its old registration before importing the WASM. The launcher and its shell installer are no longer distributed. Old launcher paths and automatic preference migration are not used by the Mods import flow; keep a copy of your old component settings if you want to migrate them manually.
