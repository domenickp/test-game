# A tiny Bevy platformer

A complete, small 2D platformer built on **Bevy 0.19**, written to be read rather than
shipped. No external crates, no art assets — every sprite is a colored rectangle and the
level is a block of ASCII art.

```
cargo run     # play it
cargo test    # 25 headless tests, no window, ~0.1s
```

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
| `src/tests.rs` | ~860 | Headless tests — also a worked example of testing a Bevy app |

Every file opens with a module comment explaining what it's for and why it's built that
way. The code is commented at roughly tutorial density, which is much heavier than you'd
want in a real project.

## A suggested reading order

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
