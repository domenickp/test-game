# A tiny Bevy platformer

A complete, small 2D platformer built on **[Bevy](https://bevy.org) 0.19**
([API docs](https://docs.rs/bevy/0.19.0/bevy/)), written to be read rather than shipped. No
external crates, no art assets — every sprite is a colored rectangle and the level is a block
of ASCII art.

**Already have Rust 1.95+?** `cargo run`. Otherwise start at [Setup](#setup).

---

## Setup

Written for someone starting with nothing installed. Three steps: install Rust, install your
platform's C toolchain, get the code and build. Ten minutes of that is typing; the rest is
waiting on the first compile, which is anywhere from 3 to 15 minutes depending on your
machine.

### 1. Install Rust

Rust is installed through [`rustup`](https://rustup.rs), which manages the compiler (`rustc`)
and the build tool (`cargo`) together. You do not need to install anything else Rust-related
— no IDE, no package manager, no separate `cargo`.

The official instructions are at
[rust-lang.org/tools/install](https://www.rust-lang.org/tools/install), and chapter 1 of *The
Rust Programming Language* covers the same ground with more explanation:
[Installation](https://doc.rust-lang.org/book/ch01-01-installation.html). The short version
for each platform:

**macOS / Linux** — paste this into a terminal and accept the default "standard
installation":

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

Then either open a new terminal or run `source "$HOME/.cargo/env"` so `cargo` is on your
`PATH`.

**Windows** — download and run [`rustup-init.exe` from rustup.rs](https://rustup.rs), or in
a terminal:

```powershell
winget install --id Rustlang.Rustup
```

Then open a new terminal. On Windows, Rust links with Microsoft's C++ toolchain; if it isn't
already present, `rustup-init` detects that and offers to install the Visual Studio Build
Tools for you. Accept — the build will fail at the link step without them.

**Check it worked.** Both commands should print a version:

```bash
rustc --version
cargo --version
```

**Bevy 0.19 requires Rust 1.95.0 or newer.** If `rustc --version` reports anything older —
likely if you had Rust installed already — update it:

```bash
rustup update stable
```

An older compiler fails with an error about the `2024` edition being unsupported, or with
`package requires rustc 1.95.0`, rather than anything that mentions this project. (Editions
are Rust's opt-in compatibility epochs — see the
[Edition Guide](https://doc.rust-lang.org/edition-guide/rust-2024/index.html) if you're
curious what 2024 changed.)

### 2. Install your platform's build dependencies

Rust brings its own compiler but not a linker, and Bevy binds to a few native libraries for
windowing, audio, and input. This step is what those need. Bevy documents it upstream too, at
[Setting up your dev environment](https://bevy.org/learn/quick-start/getting-started/setup).

**macOS** — the Xcode command line tools provide the linker:

```bash
xcode-select --install
```

If you already have Xcode or the tools installed, this tells you so and exits; nothing to
do.

**Windows** — covered by the Visual Studio Build Tools from step 1. Nothing further.

**Linux** — install the development headers for X11, ALSA, and udev. For the common distros:

```bash
# Debian / Ubuntu
sudo apt-get install g++ pkg-config libx11-dev libasound2-dev libudev-dev libxkbcommon-x11-0
sudo apt-get install libwayland-dev libxkbcommon-dev          # for Wayland sessions

# Fedora
sudo dnf install gcc-c++ libX11-devel alsa-lib-devel systemd-devel
sudo dnf install wayland-devel libxkbcommon-devel             # for Wayland sessions

# Arch / Manjaro
sudo pacman -S libx11 pkgconf alsa-lib libxcursor libxrandr libxi
```

You may also need Vulkan drivers for your GPU — `mesa-vulkan-drivers`, `vulkan-intel`, or
`vulkan-radeon`. Bevy keeps a fuller per-distro list, including Void and NixOS, in
[`linux_dependencies.md`](https://github.com/bevyengine/bevy/blob/main/docs/linux_dependencies.md).

### 3. Get the code and build

If you don't have the repository yet:

```bash
git clone https://github.com/domenickp/test-game.git
cd test-game
```

(Git comes with the Xcode tools on macOS. On Linux install it from your package manager;
on Windows, `winget install --id Git.Git`.)

Then:

```bash
cargo run
```

That's the whole build. Cargo reads `Cargo.toml`, downloads the 530 dependency crates Bevy
pulls in, compiles them, and launches the game. There are no asset files to fetch and nothing
to configure — the level is a string literal and the text uses Bevy's built-in font.

**The first build takes a while.** Measured from scratch on an M5 Pro (15 cores): **just
under 3 minutes**, plus download time. A 4-core laptop is more likely 8–15 minutes. This is
a one-time cost — Cargo caches the compiled dependencies, and after that an edit to this
project's own code rebuilds in about a second.

**It also uses real disk.** `target/` lands around **6.5 GB** after a build, and about
7.5 GB once you've run the tests and linter too. That's normal for a Bevy debug build with
debug symbols. `cargo clean` reclaims all of it whenever you want the space back.

You don't need `--release`. `Cargo.toml` sets `opt-level = 3` for dependencies in the dev
profile, so Bevy itself — the code that actually matters for frame rate — is optimized while
this project's own code still recompiles in about a second.

### Run the tests

```bash
cargo test
```

25 tests, no window, about 0.1 seconds. They exercise the real physics headlessly — a good
way to confirm your setup works even on a machine with no display.

### If something goes wrong

| Symptom | Cause and fix |
|---|---|
| `cargo: command not found` | Step 1's `PATH` change hasn't taken effect. Open a new terminal, or `source "$HOME/.cargo/env"`. |
| `feature edition2024 is required` or `requires rustc 1.95.0` | Compiler too old. `rustup update stable`. |
| `linker 'cc' not found` (macOS/Linux) | Missing C toolchain — step 2. |
| `link.exe not found` (Windows) | Visual Studio Build Tools missing. Re-run `rustup-init.exe`, or install the "Desktop development with C++" workload. |
| `Package alsa/libudev was not found` (Linux) | Missing dev headers — step 2. |
| Builds fine, then panics about a graphics adapter or surface | No usable GPU backend. On Linux install Vulkan drivers; in a VM or over plain SSH there may be no GPU at all. `cargo test` still works — it never opens a window. |

---

## Playing

**Controls** — `A`/`D` or arrows to move, `Space`/`W`/`Up` to jump, `R` to restart.

The HUD shows your run time, coins, and deaths. The clock starts on your first move or jump
— not on `R`, and not while you're getting your bearings — stops when you touch the flag,
and resets on restart.

Get to the green flag at the top right. Coins are optional. Spikes and the pit are not.

---

## What's in here

| File | Lines | What it owns |
|---|---|---|
| `src/main.rs` | ~170 | App setup. **Start here** — it lists every system in the game, in order. |
| `src/physics.rs` | ~370 | Gravity, AABB collision resolution, moving platforms |
| `src/level.rs` | ~340 | The ASCII level, and turning each character into entities |
| `src/player.rs` | ~170 | Keyboard → velocity, and the constants that are the game feel |
| `src/gameplay.rs` | ~190 | Coins, spikes, the goal flag, the score, the run timer |
| `src/camera.rs` | ~105 | Smoothed follow-camera, clamped to the level |
| `src/ui.rs` | ~190 | HUD, run-time formatting, and win overlay |
| `src/tests.rs` | ~830 | Headless tests — also a worked example of testing a Bevy app |

Every file opens with a module comment explaining what it's for and why it's built that
way. The code is commented at roughly tutorial density, which is much heavier than you'd
want in a real project.

## A suggested reading order

If you've never touched Bevy, its
[Quick Start guide](https://bevy.org/learn/quick-start/introduction) introduces the same ECS
vocabulary in about ten minutes and pairs well with this code. Then:

1. **`main.rs`** — the ECS vocabulary (entity / component / system), and the full list of
   systems in the order they run each frame. This is the map for everything else.
2. **`physics.rs`** — the axis-separated AABB collision technique. This is the one
   genuinely non-obvious algorithm in the project, and it's the thing most people get
   subtly wrong when writing a platformer from scratch. Read the `SKIN` constant's comment
   carefully: it looks like a rounding nicety and is in fact the difference between working
   moving platforms and a player who gets flung off them (see `tests.rs` for the
   regression test that caught exactly that).
3. **`player.rs`** — coyote time, jump buffering, variable jump height, asymmetric
   gravity. Four small tricks that separate a platformer that feels good from one that
   feels broken.
4. **`level.rs`** — how a character in a string becomes a pile of components.
5. Then `gameplay.rs`, `camera.rs`, `ui.rs` in any order — they're each self-contained.

## Bevy concepts each file demonstrates

- **Components as the only data store** — `physics.rs`. `Velocity` is a component, so
  `move_and_collide` works on the player without ever mentioning the player.
- **Explicit system ordering with `.chain()`** — `main.rs`. Physics ordering isn't
  optional, so it's spelled out; without it Bevy would parallelize the steps.
- **Query disjointness** (`Without<T>`) — `physics.rs`, `camera.rs`, `gameplay.rs`. Why
  Bevy sometimes refuses to compile two queries in one system, and how to fix it.
- **States and `run_if(in_state(...))`** — `main.rs`, `gameplay.rs`. Freezing the game
  loop on the win screen.
- **`DespawnOnExit`** — `ui.rs`. State-scoped entities that clean themselves up.
- **Observers and triggered events** — `level.rs`, `ui.rs`. `R` triggers `SpawnLevel`
  without knowing anything about what rebuilding a level involves.
- **Marker components for teardown** — `level.rs`. `LevelEntity` makes "restart" a
  two-line operation with nothing to forget.
- **Parent/child transforms** — `level.rs`. The goal flag is a child of its pole.
- **UI as flexbox in screen space** — `ui.rs`, and how it differs from world-space `Text2d`.
- **Headless testing** — `tests.rs`. `MinimalPlugins` plus a hand-advanced `Time` gives
  deterministic, millisecond-fast tests of the real physics.
- **Floating-point contact tolerance** — `physics.rs`. Why resting exactly on a surface is
  a knife-edge for `f32`, and why every AABB collision routine needs a skin width.
- **Clamping the timestep** — `physics.rs`. Why discrete collision plus one slow frame
  equals a player falling through the floor, and the one-line bound that prevents it.
- **Letting the schedule do the work** — `gameplay.rs`. The run timer needs no pause flag
  and no reset logic: it stops because it sits in a state-gated chain, and resets because
  `Progress` is replaced wholesale on rebuild. Only *starting* it needed real code.

## Things to try

Ordered roughly by effort.

**Tuning (edit a constant, run):**

- Set `COYOTE_TIME` and `JUMP_BUFFER` to `0.0` in `player.rs`. Play for a minute. This is
  the single most instructive experiment in the repo — the game becomes noticeably worse
  in a way that's hard to name if you've never seen these two tricks.
- Set `FALL_GRAVITY_MULTIPLIER` to `1.0` in `physics.rs` for "realistic" symmetric
  gravity, and notice how floaty it feels.
- Set `CAMERA_SMOOTHING` in `camera.rs` to `2.0`, then `40.0`.

**Level design (edit the ASCII in `level.rs`):**

- Move some coins, add a spike, widen a gap.
- Note that the jump reaches `JUMP_SPEED² / (2·|GRAVITY|)` ≈ 107 units ≈ 3.3 tiles high
  and about 5 tiles across at running speed. That number is what makes the current gaps
  clearable; change `JUMP_SPEED` or `GRAVITY` and you've changed the level's difficulty.

**New features (write some code):**

- **A new tile type.** Add a character to `LEVEL` and a `match` arm in `spawn_level`.
  Nothing else needs to change.
- **A double jump.** Add an `air_jumps_left: u32` field to `Player`, refill it in
  `player_input` when grounded, and spend one when a jump fires with no coyote time left.
- **Wall jumping.** `move_and_collide` already knows when it zeroed X velocity against a
  wall — record which side in a component and read it in `player_input`.
- **Sound.** `commands.spawn(AudioPlayer::new(asset_server.load("coin.ogg")))` in
  `collect_coins`. You'll need an `assets/` directory.
- **A real sprite.** Swap `Sprite::from_color(...)` for
  `Sprite::from_image(asset_server.load("player.png"))` and the rest of the game is
  unchanged — the collider is separate from how the entity is drawn, on purpose.

## Deliberate simplifications

Worth knowing about, since they're the places this diverges from what you'd do for real:

- **Physics runs in `Update`, not `FixedUpdate`.** So the timestep varies with frame rate.
  Everything is written to be frame-rate independent (velocities are per-second, `move_toward`
  and the camera easing both scale by the frame delta), and the delta itself is clamped to
  `MAX_TIMESTEP` so a long frame can't move anything far enough to tunnel through a floor.
  The cost: a frame slower than 1/30s runs in slow motion instead of skipping ahead.
  `FixedUpdate` handles this properly by taking as many fixed steps as needed to catch up,
  at the price of needing visual interpolation to stay smooth — more correct, and more
  machinery than belongs in a first read.
- **Collision is brute-force.** Every mover is tested against every solid, every frame.
  That's ~1000 checks here and completely free; a large level wants a spatial grid.
- **One level, in a `const`.** A real game loads levels as assets so they can change
  without recompiling.
- **`main.rs` registers every system directly.** Bevy's `Plugin` trait is the usual way to
  group related systems, but it adds a layer of indirection that hurts a first read — the
  whole point of `main.rs` here is that one screen shows you everything that runs.

---

## Further reading

Everything below is the documentation I actually used while writing this, so it's the same
set you'd want open while changing it.

**Bevy**

| Link | What it's for |
|---|---|
| [Quick Start guide](https://bevy.org/learn/quick-start/introduction) | The official intro. Start here if ECS is new to you. |
| [`bevy` 0.19 API docs](https://docs.rs/bevy/0.19.0/bevy/) | Pinned to the exact version this project uses. Bevy's API moves fast, so prefer this over `latest` — and over anything you find in a blog post. |
| [Official examples](https://github.com/bevyengine/bevy/tree/latest/examples) | ~200 focused examples. The fastest way to answer "how do I do X in Bevy". |
| [Migration guides](https://bevy.org/learn/migration-guides/) | What changed between releases. Essential when a tutorial written for 0.14 doesn't compile. |
| [Community assets & learning](https://bevy.org/assets/#learning) | Tutorials, books, and plugin crates. |
| [Discord](https://discord.gg/bevy) / [GitHub Discussions](https://github.com/bevyengine/bevy/discussions) | Where to ask. Both are friendly to beginners. |

A useful trick: the examples ship *inside* the crate you already downloaded, so you can read
the ones matching your exact version offline —
`~/.cargo/registry/src/*/bevy-0.19.0/examples/`. That's where the `2d/` and `ui/` directories
came in handy while building this.

**Rust**

| Link | What it's for |
|---|---|
| [The Rust Programming Language](https://doc.rust-lang.org/book/) | "The book". The standard way to learn the language. |
| [Rust by Example](https://doc.rust-lang.org/rust-by-example/) | Same material, runnable snippets instead of prose. |
| [Standard library docs](https://doc.rust-lang.org/std/) | Reference for `Option`, `Result`, iterators, and friends. |
| [The Cargo Book](https://doc.rust-lang.org/cargo/) | `Cargo.toml` fields, profiles, features, workspaces. |
| [Edition Guide](https://doc.rust-lang.org/edition-guide/rust-2024/index.html) | What the `2024` edition in `Cargo.toml` means. |

**This project**

```bash
cargo doc --no-deps --open
```

Renders the module and item documentation from this codebase as browsable HTML in about a
second. Since every file, system, and constant here carries a doc comment explaining *why*
it's built that way, this is a genuinely readable second view of the project — and it's how
the intra-doc links in the source (``[`SKIN`]``, ``[`MAX_TIMESTEP`]``, and so on) are meant
to be followed: they become real hyperlinks here.

Drop `--no-deps` to document all 530 dependencies too, but expect it to take a few minutes
and add a lot to `target/`. Reaching for [docs.rs](https://docs.rs/bevy/0.19.0/bevy/) instead
is usually the better trade.
