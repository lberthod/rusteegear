# Quick start — about 30 minutes the first time

Goal: see a playable scene in the editor, with no decision to make. Every
command can be copied as is. French version: [QUICKSTART.md](QUICKSTART.md).

Realistic budget: **5 minutes of hands-on work**, plus what the tools do on
their own — installing rustup and Git LFS, cloning (~240 MB), downloading the
pinned 1.98.0 toolchain (~200 MB) and **the first compilation (~5 minutes on
an Apple M4, longer on an older machine)**. Later launches take seconds.

## 1. Prerequisites

Rust via [rustup](https://rustup.rs). If you do not have it:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

[Git LFS](https://git-lfs.com) — `assets/models/` (482 `.glb` models plus
344 PNG previews) is tracked by Git LFS:

```bash
brew install git-lfs   # or apt/dnf/pacman depending on your distribution
git lfs install
```

Without it, `git clone` fetches text pointers instead of the real `.glb`
files — `doctor.sh` (next step) detects that.

Linux: install the system libraries `libx11-dev libxkbcommon-dev
libwayland-dev libasound2-dev libudev-dev libdbus-1-dev pkg-config`
(Debian/Ubuntu names). Windows: nothing beyond rustup and Git LFS.

## 2. Clone and check the environment

```bash
git clone https://github.com/lberthod/rusteegear
cd rusteegear
rustup update
./scripts/doctor.sh
```

`doctor.sh` must print "Environnement prêt" (environment ready). Otherwise it
prints the repair command for each ✗.

## 3. Launch the editor

```bash
cargo run --profile dev-fast
```

⏱️ **The first compilation takes ~5 minutes** (measured on an Apple M4).
Later launches recompile in **seconds**. This is normal Rust behaviour, not an
installation problem.

At startup the console prints:

```text
RusteeGear 0.1.0
GPU : <your card> (<Metal|Vulkan>)
```

and the editor opens on the demo scene (the game's hamlet).

The editor is validated on macOS. Linux and Windows are compiled in CI (and
the editor is launched under Xvfb on Linux), but nobody has run them by hand
yet — reports welcome.

## 4. Open the example project

1. Menu **📂 Ouvrir un projet…** (Open a project)
2. Select the folder `examples/first_game` (inside the cloned repository)
3. Click **Play**

## 5. Play

- **Arrows / WASD**: move the player (the orange capsule)
- **Space**: jump
- Walk on the **yellow zone**: it turns green
- Collect the **3 golden coins**: the goal of the scene

**Stop** brings the scene back exactly to its pre-Play state.

## 6. Move around the level

- Left click + drag: orbit the camera. Wheel: zoom.
- **Right click held**: fly camera — mouse to look around, `W A S D` to move,
  `E`/`Q` up and down, `Shift` for speed. A right click without dragging
  opens the context menu.

## What next?

- **Build something yourself (10 min)**: [docs/FIRST_GAME.md](docs/FIRST_GAME.md) (French)
- **Understand the engine (1 page)**: [docs/MENTAL_MODEL.md](docs/MENTAL_MODEL.md) (French)
- The example project's content: [examples/first_game/README.md](examples/first_game/README.md)

## Going further

- **All controls** (keyboard, mouse, game, touch, gamepad):
  [docs/CONTROLS.md](docs/CONTROLS.md) — also in **Aide › ⌨ Raccourcis clavier**.
- **Drive the application from a script or an agent** (`--pilot`):
  [docs/PILOT.md](docs/PILOT.md).
- **Contribute** (clippy, tests, what CI checks before you push):
  [CONTRIBUTING.md](CONTRIBUTING.md).
- **Full comparison with Unity, Unreal, Godot and Bevy**:
  [docs/ANALYSE_COMPARATIVE_MOTEURS_2026-09-04.md](docs/ANALYSE_COMPARATIVE_MOTEURS_2026-09-04.md).

## Express troubleshooting

| Symptom | Likely cause |
| --- | --- |
| `error: package … requires rustc 1.x` | `rustup update` |
| very long compilation | normal the first time (see §3) |
| verbose logs wanted | `RUST_LOG=debug cargo run --profile dev-fast` |
| player mode (welcome screen: nickname, online / solo) | `cargo run --profile dev-fast -- --player` |
| play without network, no welcome screen | `RUSTEEGEAR_OFFLINE=1 cargo run --profile dev-fast -- --player` |
| black window on Linux | try `WGPU_BACKEND=vulkan`, and check that a Vulkan driver (or `mesa-vulkan-drivers`) is installed |
