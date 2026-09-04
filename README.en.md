<div align="center">

# 🦀 RusteeGear

**A compact Rust engine to build and ship small multiplayer 3D games — desktop, mobile and web — with an authoritative server included.**

winit · wgpu · egui — no third-party engine.

![CI](https://github.com/lberthod/rusteegear/actions/workflows/ci.yml/badge.svg?branch=main)
![language](https://img.shields.io/badge/Rust-1.98-orange?logo=rust)
![platforms](https://img.shields.io/badge/macOS%20·%20Android%20·%20iOS%20·%20Web-running-success?logo=apple)
![rendering](https://img.shields.io/badge/wgpu-Metal%20%7C%20Vulkan%20%7C%20WebGPU-blue)
![license](https://img.shields.io/badge/license-MIT-green)

🇫🇷 The French README is the reference and stays more detailed: [README.md](README.md).

### 🎮 [Try the demo in your browser](https://lberthod.github.io/rusteegear/)

No install: WebGPU (recent Chrome/Edge), keyboard (WASD + Space/J/K/H),
connected to the **same multiplayer server** as the desktop and Android
builds — everyone who opens the link lands in the same match.
API docs: [/doc/](https://lberthod.github.io/rusteegear/doc/motor3derust/).

</div>

---

## Contents

- [Vision](#vision)
- [What RusteeGear is for](#what-rusteegear-is-for)
- [Why Rust, and why not Bevy](#why-rust-and-why-not-bevy)
- [Features available today](#features-available-today)
- [Asset pipeline: Blender → GLB](#asset-pipeline-blender--glb)
- [Online multiplayer](#online-multiplayer)
- [Quick start](#quick-start)
- [External piloting (agents, scripts, CI)](#external-piloting-agents-scripts-ci)
- [Architecture](#architecture)
- [Where it stands against Unity, Unreal, Godot and Bevy](#where-it-stands-against-unity-unreal-godot-and-bevy)
- [Tech stack](#tech-stack)
- [License](#license)

> **New here? Three documents, in this order:**
> 1. **[QUICKSTART.en.md](QUICKSTART.en.md)** — run the editor and play the example project (5 minutes plus compile time);
> 2. **[docs/FIRST_GAME.md](docs/FIRST_GAME.md)** (French) — build your first animated object (10 minutes);
> 3. **[docs/MENTAL_MODEL.md](docs/MENTAL_MODEL.md)** (French) — how the engine works, on one page.
>
> Before reporting a problem: **[docs/KNOWN_LIMITATIONS.md](docs/KNOWN_LIMITATIONS.md)**
> (support matrix per platform, deliberate limits of the preview).
> Most of `docs/` is written in French; the code comments are French too.

---

## Vision

RusteeGear is a compact Rust engine to **build and ship small multiplayer 3D
games**, from prototype to a playable match with friends. What sets it apart:

1. **A light visual editor** — scene, hierarchy, inspector, gizmos: build a
   level without boilerplate.
2. **Runtime and server written in Rust** — one language, one set of
   guarantees, from the client to the server.
3. **The same simulation in solo, on the client and on the server** —
   physics, combat, scene: one game code, nothing duplicated or resynced by hand.
4. **Web, mobile and desktop export** from the same code base.
5. **An understandable architecture** — every layer (window → events → state
   → GPU rendering → network) is written by hand, no black box.
6. **No ECS to learn** — the scene is a plain `Vec<SceneObject>`.
7. **A complete project one person can hold in their head** — about 67,000
   lines of Rust (excluding tests), from rendering to networking.

---

## What RusteeGear is for

Building and shipping a **small multiplayer 3D game** — a co-op arena, a wave
shooter, a mini dungeon, a connected teaching experience — should not require
swallowing a multi-million-line engine or a full ECS just to put four players
in the same scene. Mainstream engines (Unity, Unreal, Godot) are extraordinarily
complete but opaque: a runtime you cannot read, a licensing and telemetry model
you live with, and multiplayer you mostly assemble yourself on top. A from-scratch
ECS engine like Bevy imposes its own learning curve before the first line of gameplay.

RusteeGear targets a precise need:

- **Ship fast, on several platforms.** The same project exports to
  **desktop** (macOS `.dmg`), **mobile** (Android `.apk`, iOS) and **web**
  (WASM/WebGPU, playable without install).
- **Multiplayer without duplicated logic.** The included **authoritative
  server** (`src/bin/server.rs`) runs exactly the same simulation as the client.
- **Full control, no ECS to learn.** Each stage of the pipeline is readable in
  an afternoon.
- **Hackable and minimal.** Adding a primitive, a collider type or a script
  variable takes a few lines.
- **No heavy dependency, no hidden runtime.** No garbage collector, no embedded
  engine, no license to negotiate — one native binary.

RusteeGear does not try to compete with Unity or Unreal on feature breadth or
photorealism: it is aimed at **small multiplayer 3D games**, where portability,
lightness and code you master end to end matter more than a rich asset store.

---

## Why Rust, and why not Bevy

Rust gives native, predictable performance (no GC pauses), memory safety
without runtime cost (glTF import, audio decoding and scene loading run on
background threads, guaranteed by `Send`/`Sync`), a first-class graphics
ecosystem (`wgpu`, `winit`, `egui`, `glam`, `rapier3d`, `kira`) and real
portability: the same core compiles to macOS, an Android `.so`, an iOS binary
and a WASM/WebGPU target.

[Bevy](https://bevyengine.org/) is the excellent Rust engine: full ECS, system
scheduler, PBR rendering, plugins, a large ecosystem. For a broad range of games
or a team, Bevy (or Godot, Fyrox) is perfectly legitimate, often preferable.
RusteeGear aims elsewhere: shipping **small multiplayer 3D games** with code you
fully own — no ECS, no scheduler, no rendering framework to learn first. It only
depends on **targeted, replaceable bricks** (`winit`, `wgpu`, `egui`,
`rapier3d`, `kira`, `mlua`) and **assembles everything else itself**: event
loop, render pipeline, picking, gizmos, serialization, networking, Play mode.

| Criterion | RusteeGear (from scratch) | Bevy |
|---|---|---|
| Goal | Small multiplayer 3D games, readable code | Any game, rich ecosystem |
| Core size | ~67k lines, readable end to end | Very large, many subsystems |
| Architecture | Scene = `Vec<SceneObject>`, explicit | Full ECS + system scheduler |
| Rendering | Hand-written `wgpu`/WGSL pipeline | Built-in renderer (PBR…) |
| Learning curve | Read the code directly | Learn the framework first |
| Control | Total | Framed by the engine's conventions |

---

## Features available today

**Rendering**
- Real-time 3D via `wgpu`, WGSL shaders, HDR target (`Rgba16Float`), MSAA 4×.
- **Cascaded shadow maps** (3 cascades, texel-snapped, 5×5 PCF) from the
  directional light — large maps without blurry shadows.
- **Transparency**: per-object `opacity`, a sorted back-to-front alpha-blended
  pass after the opaque geometry (water, glass, ghosts, death fade).
- Per-object materials (metallic / roughness / emissive) + specular; albedo textures with mipmaps.
- Lights: global directional + ambient, **point lights and spots** (up to 8, nearest to the camera).
- **Instanced rendering** (one draw per mesh+texture batch), CPU frustum culling, distance culling, foliage impostor LOD.
- Allocation-free frame path; vsync + adaptive cadence.
- **Skeletal animation**: skinned glTF import, GPU skinning, cross-fade between
  clips (`obj.anim` from Lua), **automatic locomotion** (idle / walk / run
  blended from the measured speed — three clips, zero script), replicated online.
- Sky gradient + exponential fog, bloom, ACES tone mapping.

**Editing**
- Primitives (cube / sphere / plane / cylinder / capsule / terrain) + async glTF/GLB import.
- `egui` editor: toolbar (Play/Pause/Stop), hierarchy (groups, drag and drop, filter, inline rename), inspector, status bar, toasts.
- Selection by 3D click or hierarchy, multi-selection; translate / rotate / scale gizmos (multi-object, common pivot), snapping.
- Align / distribute, group / ungroup; **undo / redo** for everything including inspector edits and glTF import; cut / copy / paste / duplicate.
- **Fly camera** on right-click hold (mouse look, WASD, E/Q up/down, Shift for speed), Unity/Godot style.
- Asset manager (`asset://`, `asset-id://` stable references), prefabs with per-instance overrides, JSON scenes with schema versioning.
- Integrated Lua script editor with **breakpoints**, console, CPU/GPU/memory profiler, crash log, asset hot reload.

**Game runtime** (Play ▶ / Pause ⏸ / Stop ⏹, resettable preview)
- **Physics** `rapier3d`: static / dynamic / kinematic bodies, explicit colliders
  (auto, box, sphere, capsule, convex hull, exact trimesh), heightfield terrain,
  CCD, collision layers, kinematic character controller with game-feel tuning.
- **Joints** from the inspector: fixed / revolute (with limits) / spherical, to another object or to the world — doors, bridges, pendulums, no code.
- **Trigger zones as rapier sensors**: `obj.overlapped`, `obj.overlap_count`,
  `obj.overlap_names` react to any physics body (crate, creature, player) — pressure plates, traps, drop zones.
- Raycast and sphere overlap queries exposed to Lua.
- **Audio** `kira`: music/SFX buses, cross-fade, ducking, reverb/EQ/compressor, distance attenuation + stereo panning, streaming (native), per-trigger pitch/volume randomization.
- Game camera + automatic player follow; camera collision.

**Lua scripting API** (`mlua` Lua 5.4 natively, `rilua` Lua 5.1 on the web, differential tests between both)
```lua
obj.x/y/z   obj.rx/ry/rz   obj.sx/sy/sz   obj.r/g/b
obj.tapped, obj.touch_started, obj.touching, obj.touch_ended
obj.triggered, obj.exited                 -- the player entered / left this trigger zone
obj.overlapped, obj.overlap_count, obj.overlap_names  -- any physics body inside (rapier sensor)
obj.anim = "run"                          -- change the clip (skinned objects), automatic cross-fade
dt, time, input.jx, input.jy, input.btn.<name>, tilt.x, tilt.y
vibrate(ms), reverb(mix), set_health(0..1), damage(v), add_item(kind, n)
spawn(prefab_ref, x, y, z), find_tag(tag), emit(name), on_event(name)
save.get(key), save.set(key, value), raycast(...), overlap_sphere(...), debug.line(...)
```
Portability between native and web: [docs/LUA_PORTABLE.md](docs/LUA_PORTABLE.md).

**Mobile** — device preview (phone frame, portrait/landscape), virtual joystick + buttons, trigger zones, health bar; macOS `.dmg`, Android `.apk`, iOS player.

**Gamepad** (`gilrs`) — left stick movement, mapped actions, persisted remapping.

**Declarative HUD** — text / image / gauge / button widgets stored in the scene, bound to game values, editable without code.

**AI (DeepSeek, experimental)** — generate or optimize a Lua script or a whole scene from a prompt.

**Tools** — console, profiler, APK readiness check, mobile optimization (texture downscale, light budget), system diagnostic, opt-in crash log, external piloting bridge (`--pilot`).

**Demos** — `File → 🎬 Demos`: first game tutorial, MMORPG demo, playable games (zombies, dungeon, tower, endless runner, duel), multiplayer modes (waves, survival, boss, escort), technical examples.

---

## Asset pipeline: Blender → GLB

All 3D content (**482 `.glb` files** in `assets/models/`) comes from a
**procedural, reproducible, scripted** pipeline: ~60 Python scripts drive
Blender headless (`scripts/blender/gen_*.py`), apply a strict art direction
(≤ 3 hues per object, no textures, `base_color_factor` only, Y-up export) and
write the `.glb` plus an optional preview PNG. The engine imports glTF
asynchronously (static or skinned), caches each mesh once, and a standalone
GLB manager (`cargo run --bin glbviewer`) browses the whole catalogue.

---

## Online multiplayer

RusteeGear is playable **online**, in a round-based co-op mode (Waves /
Survival / Escort / Boss) with player classes and a daily contract. The first
real game built on it, **The Hamlet of Embers**, is a 2–16 player co-op where
fairy Watchers defend a fortified village each "night".

```
┌─────────────┐        WebSocket (bincode)        ┌──────────────────────────┐
│   Client     │ ───────────────────────────────▶  │   src/bin/server.rs      │
│ (desktop,    │  ClientMsg::Join / Input / Leave  │   (headless, no GPU)     │
│  mobile, web)│ ◀───────────────────────────────  │                          │
└─────────────┘   ServerMsg::Welcome / Snapshot /  │  AppState — the SAME as  │
                   PlayerJoined / PlayerLeft /      │  the desktop editor +    │
                   Event                            │  app::multiplayer        │
                                                     └──────────────────────────┘
```

- **Headless server**: the same `AppState` as the editor (scene, physics, combat), no window, no GPU.
- **Each network player is its own independently driven object**; the server validates movement, cooldowns, weapons, healing.
- **Compact protocol** (`bincode`): a 20-entity snapshot fits in ~540 bytes. Rate limiting, frame size cap, connections per IP.
- **Smooth movement despite latency**: remote entities interpolated with a render delay, local player predicted immediately, **trajectory-history reconciliation** (a server position on the predicted path is merely late, not an error).
- **Rooms** (16 rooms, 256 connections), classes (Assault / Scout / Support), individual health, kills and assists, XP.
- **Firebase Realtime Database** as a side backend (accounts, lobby chat, leaderboard) — never the real-time gameplay.

```bash
cargo run --bin server                                   # listens on 127.0.0.1:7777
RUSTEEGEAR_SERVER_ADDR=0.0.0.0:9000 cargo run --bin server
```

Known limits: no PvP damage, no matchmaking or public room list (two groups can pick the same room code by mistake).

---

## Quick start

```bash
cargo run --profile dev-fast                # desktop editor (playable profile, see QUICKSTART.en.md)
cargo run --profile dev-fast -- --player    # player mode (full-screen scene, welcome screen)
```

Per-platform builds:
```bash
./packaging/build_dmg.sh            # macOS .dmg (cargo install cargo-bundle)
./packaging/build_apk.sh            # Android .apk (NDK + cargo install cargo-apk)
./packaging/install_ios_device.sh   # iPhone plugged in (Xcode + xcodegen)
./packaging/build_web.sh            # WASM + WebGPU demo
```

No target is signed for store distribution. Editor controls: [docs/CONTROLS.md](docs/CONTROLS.md).

| Action | Command |
|---|---|
| Orbit the camera | left click + drag on the 3D view; `T` for free orbit |
| Fly camera | **right click held** + mouse; `W A S D`, `E`/`Q` up/down, `Shift` fast |
| Zoom / pan | wheel / middle click (or `Shift`) + drag |
| Select | click on the object, or in the hierarchy |
| Add | **Add** menu (cube, sphere, plane…) or **Add ▸ cards** |
| Play / Stop | ▶ Play / ⏹ Stop (the scene returns to its pre-Play state) |
| Save / open | `Cmd+S` · `Cmd+Shift+S` · `Cmd+O` · `Cmd+N` |

---

## External piloting (agents, scripts, CI)

A winit/wgpu window has no accessibility tree, so no screen-control tool can
drive it. Instead, on explicit request, the app exposes a **local TCP bridge**
giving access to its semantics: developer console commands, Lua eval, scene
inspection, player input injection, screenshots and logs — for an agent, an
audit script or CI.

```bash
cargo run -- --pilot                        # the app, bridge on 127.0.0.1:4517
cargo run --bin pilot -- console play       # start Play mode
cargo run --bin pilot -- lua "return 1+1"   # evaluate Lua inside the live engine
cargo run --bin pilot -- screenshot /tmp/s.png
cargo run --bin pilot -- console stop
```

Never on by default, localhost only, ~13 ms per command, tested without a GPU
in CI. Protocol and limits: [docs/PILOT.md](docs/PILOT.md) (French).

---

## Architecture

```
src/
├── lib.rs         # winit event loop + run() (desktop) + android_main (cdylib) + mobile resume
├── main.rs        # desktop entry → motor3derust::run()
├── bin/server.rs  # headless game server (multiplayer) — no gfx/egui/winit
├── bin/pilot.rs   # CLI client of the piloting bridge
├── bin/glbviewer.rs # standalone GLB catalogue browser
├── assets.rs      # embedded assets (include_dir, bundle:// scheme) for exported players
├── app/           # GPU-free logic: AppState, simulation, picking, selection, scripting, combat, multiplayer
├── gfx/           # wgpu rendering (renderer, meshes, camera, cascades, transparency, WGSL shaders)
├── scene/         # Transform, MeshKind, Scene, components, glTF import, prefabs, serialization
├── runtime/       # Play mode: physics (rapier3d: bodies, colliders, sensors, joints), audio (kira), save games
├── net/           # multiplayer: protocol (bincode), server loop / client (WebSocket), interpolation
└── editor/        # egui UI (toolbar, hierarchy, inspector, HUD, export panel) — desktop
```

Clear split **logic (`app`) / rendering (`gfx`)**: state never depends on the
GPU, which made the mobile port direct and lets `src/bin/server.rs` reuse the
exact same simulation headless. Details: [docs/architecture.md](docs/architecture.md) (French).

---

## Where it stands against Unity, Unreal, Godot and Bevy

A full comparison (13 feature grids, maturity radar, recommendations) lives in
[docs/ANALYSE_COMPARATIVE_MOTEURS_2026-09-04.md](docs/ANALYSE_COMPARATIVE_MOTEURS_2026-09-04.md)
(French). In short:

- **Networking** is where RusteeGear is genuinely competitive: same simulation
  on client and server, prediction, interpolation and trajectory-history
  reconciliation out of the box — above Godot and Bevy, only Unreal does
  better natively.
- **Testing and piloting** exceed the four engines for a project of this size:
  800+ tests, headless render goldens, native/web Lua differential tests, a
  panic budget enforced in CI, and the `--pilot` bridge.
- **Rendering and the scene model** are still prototype-grade: no real PBR
  BRDF, no particles, no parent/child transform hierarchy, a thirty-field
  `SceneObject`. The September 2026 short-term batch closed the most visible
  gaps (cascaded shadows, transparency, joints, sensors, locomotion blend, fly
  camera, Linux/Windows CI, undo everywhere).

---

## Tech stack

| Need | Crate |
|---|---|
| Window / events | `winit` |
| GPU rendering | `wgpu` (WGSL) |
| Math | `glam` |
| Editor UI | `egui` + `egui-wgpu` + `egui-winit` |
| Serialization | `serde` + `serde_json` |
| 3D import | `gltf` |
| Scripting | `mlua` (Lua 5.4) native · `rilua` (Lua 5.1) web |
| Physics | `rapier3d` |
| Audio | `kira` |
| Embedded assets (player) | `include_dir` + `zstd`/`ruzstd` |
| Networking | `tokio` + `tokio-tungstenite` (WebSocket) · `web-sys` on the web · `bincode` |
| Packaging | `cargo-bundle` (macOS) · `cargo-apk` (Android) · `xcodegen` + Xcode (iOS) · `wasm-bindgen` (web) |

---

## License

MIT — see [LICENSE](LICENSE). Do what you want with it. 🦀
