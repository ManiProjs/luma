# Installation

Luma's installer creates a self-contained user installation for the Rust Core,
Ink terminal client, embedded dashboard, and browser automation runtime. It
does not require Rust, Bun, Node.js, or a project checkout to be present first.

## macOS and Linux

```sh
curl -fsSL https://raw.githubusercontent.com/ManiProjs/luma/main/scripts/install.sh | bash
luma setup
```

The installer detects the current architecture, downloads the matching Luma
bundle, verifies its SHA-256 checksum, and installs under
`~/.local/share/luma`. The `luma` and `luma-core` commands are placed in
`~/.local/bin`; the installer adds that directory to `~/.zprofile` for zsh,
`~/.bash_profile` for macOS Bash, or `~/.profile` otherwise. Open a new terminal
after installation.

It also installs pinned Node.js, Bun, Playwright, Chromium, and a platform
build of ripgrep. On Debian/Ubuntu, it installs Chromium's system libraries
and Git when missing. On macOS, Git must be available through the Xcode
Command Line Tools. Other Linux distributions need Git and Chromium's shared
libraries installed by their system package manager before browser automation
can run.

Supported shell targets:

- `aarch64-apple-darwin`
- `x86_64-apple-darwin`
- `aarch64-unknown-linux-gnu`
- `x86_64-unknown-linux-gnu`

## Windows

Run in PowerShell:

```powershell
irm https://raw.githubusercontent.com/ManiProjs/luma/main/scripts/install.ps1 | iex
luma setup
```

Windows x64 is supported. The installer uses `winget` to install Git for
Windows if it is not already available. Open a new terminal after setup so the
user PATH update takes effect.

## Installed layout

```text
~/.local/share/luma/
├── bin/luma-core
├── lib/luma-cli.js
├── runtime/
│   ├── node/
│   └── node_modules/       # Bun, Playwright, and platform ripgrep
├── bundle.json
└── LICENSE
```

The dashboard's production assets are embedded in `luma-core`. Playwright's
Chromium download is stored in `~/.cache/ms-playwright` on macOS/Linux and in
`%LOCALAPPDATA%\ms-playwright` on Windows. The installer copies the bundled
`ask-question` skill into the user's Luma skills directory without replacing
an existing copy.

## Updating and uninstalling

Run the installer again to update to the newest published release. Set
`LUMA_VERSION=vX.Y.Z` to pin a release. Existing user configuration and
installed skills are preserved.

```sh
curl -fsSL https://raw.githubusercontent.com/ManiProjs/luma/main/scripts/install.sh | bash -s -- --uninstall
```

On Windows, save `install.ps1` and run it with `-Uninstall`. Uninstall removes
the program and its private runtimes but deliberately preserves user data.

The Electron desktop application is distributed separately from this
terminal/runtime installer.
