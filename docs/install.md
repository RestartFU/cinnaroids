# Install Cinnaroids

Linux and macOS:

```sh
curl -fsSL https://github.com/RestartFU/cinnaroids/releases/latest/download/install.sh | sh
```

The installer selects the latest stable release for your OS and architecture, verifies its SHA-256 checksum, and installs without sudo. Linux x86_64 installs to `~/.local/bin/cinnaroids`; macOS Intel and Apple Silicon install to `~/Applications/Cinnaroids.app`. Run the same command again to update. Existing launcher settings and module preferences are preserved.

On Linux, run `cinnaroids` (or `~/.local/bin/cinnaroids` if `~/.local/bin` is outside your `PATH`). On macOS, open `~/Applications/Cinnaroids.app`. Linux requires Vulkan drivers, Fontconfig, Wayland or X11, and xkbcommon. The macOS app is ad-hoc signed and not notarized; use **Open Anyway** in System Settings if macOS blocks it.

To install a specific version:

```sh
curl -fsSL https://github.com/RestartFU/cinnaroids/releases/latest/download/install.sh | sh -s -- --version 2.6.2
```

Windows: download and extract the `windows-x86_64.zip` archive, then run `Cinnaroids.exe`.
