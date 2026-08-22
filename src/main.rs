//! # A tiny 2D platformer, built to be read
//!
//! This is a complete (if small) platformer written against **Bevy 0.19** with no
//! external crates and no art assets — every block is a flat-colored box, and the level
//! is a block of ASCII art in [`level::LEVEL`].
//!
//! The gameplay is entirely two-dimensional: [`physics`] resolves 2D AABBs, and
//! [`level::LEVEL`] is a flat grid. It is *drawn*, however, with 3D meshes seen through
//! a long-lens perspective camera — see [`camera::CAMERA_FOV`] for why that looks
//! exactly like a 2D game, and [`render`] for how a 2D level grows a third dimension.
//!
//! ## The Bevy ideas this project demonstrates
//!
//! Bevy is an **ECS** (Entity Component System) engine. There are only three nouns:
//!
//! * **Entity** — an id. Nothing more. The player is an entity; so is each coin.
//! * **Component** — a plain Rust struct attached to an entity. `Velocity`, `Coin`,
//!   `Transform`. Components are the *only* place game data lives.
//! * **System** — a plain Rust function. Bevy looks at the function's parameters and
//!   automatically hands it whatever it asked for. A `Query<&mut Transform>` parameter
//!   means "give me every entity that has a `Transform`, mutably".
//!
//! You never call a system yourself. You *register* it into a **schedule**
//! ([`Startup`], [`Update`], ...) and Bevy runs it at the right time. That registration
//! happens in [`main`] below, which is deliberately the only place in the codebase
//! that knows the full list of systems — read `main` first and you have the whole map.
//!
//! ## Where to look for what
//!
//! | File | What it owns |
//! |---|---|
//! | [`level`] | The ASCII level, and turning each character into entities |
//! | [`render`] | The palette, and giving each entity a body to draw |
//! | [`physics`] | Gravity, AABB collision resolution, moving platforms |
//! | [`player`] | Reading the keyboard and turning it into velocity (the "game feel") |
//! | [`gameplay`] | Coins, spikes, the goal flag, the score |
//! | [`camera`] | A smoothed follow-camera clamped to the level |
//! | [`ui`] | The HUD and the "you win" overlay |
//!
//! ## Things to try changing
//!
//! * Edit the ASCII art in `level.rs` — that is the whole level editor.
//! * Tweak the `const`s at the top of `player.rs`. They *are* the game feel.
//! * Set `COYOTE_TIME` and `JUMP_BUFFER` to `0.0` and notice how much worse it feels.
//! * Add a new character to the level and a new `match` arm in `spawn_level`.

// Bevy `Query` types are legitimately verbose — `Query<(&Transform, &Collider),
// (With<Hazard>, Without<Player>)>` is about as short as that idea gets. Clippy's
// `type_complexity` lint fires on nearly every interesting query, so Bevy projects
// (including Bevy's own examples) turn it off crate-wide rather than sprinkling
// `#[allow]` everywhere or hiding queries behind type aliases nobody can read.
#![allow(clippy::type_complexity)]

mod camera;
mod gameplay;
mod level;
mod physics;
mod player;
mod render;
mod ui;

/// Headless tests for the game logic — `cargo test`. Worth a read: they double as a
/// demonstration of how to drive a Bevy app without a window.
#[cfg(test)]
mod tests;

use bevy::prelude::*;

/// The size of one level tile, in world units.
///
/// Bevy has no built-in notion of "pixels" in the world — a `Transform` is measured in
/// abstract world units, and the camera decides how many of them fit on screen (see
/// [`VIEW_HEIGHT`]). We happen to set things up so one world unit ends up roughly one
/// pixel, but nothing in the code depends on that.
pub const TILE: f32 = 32.0;

/// How many world units tall the camera's view is, always, at any window size.
///
/// 448 / [`TILE`] = 14 tiles of vertical view. Because the camera keeps this fixed and
/// only widens or narrows horizontally, resizing the window never changes how far the
/// player can see vertically — which matters a lot in a platformer.
pub const VIEW_HEIGHT: f32 = 448.0;

/// High-level app states.
///
/// A `States` enum is Bevy's built-in way to say "only run these systems right now".
/// We use it in two ways below: `run_if(in_state(...))` to gate the whole gameplay
/// loop, and `OnEnter(...)` to spawn the win screen exactly once on transition.
#[derive(States, Debug, Clone, Copy, Default, Eq, PartialEq, Hash)]
pub enum GameState {
    /// Normal play: physics runs, input is read.
    #[default]
    Playing,
    /// The player touched the flag. Physics is frozen and an overlay is shown.
    Won,
}

fn main() {
    App::new()
        // `DefaultPlugins` is the engine: windowing, rendering, input, assets, time,
        // audio, UI. `.set(...)` swaps out one plugin's config while keeping the rest.
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Bevy Platformer".into(),
                resolution: (1280, 720).into(),
                ..default()
            }),
            ..default()
        }))
        // Resources are global singletons. `ClearColor` is the background color the
        // renderer wipes each frame with, and it's just a resource we can overwrite.
        .insert_resource(ClearColor(Color::srgb(0.07, 0.08, 0.13)))
        // `init_state` registers the state machine *and* inserts `State<GameState>` /
        // `NextState<GameState>` resources, starting at the `#[default]` variant.
        .init_state::<GameState>()
        // `init_resource` inserts `T::default()`. All three of these are written to
        // while the level is being built, and read during play.
        .init_resource::<gameplay::Progress>()
        .init_resource::<level::PlayerSpawn>()
        .init_resource::<level::LevelBounds>()
        // Shared mesh and material handles, so a thousand tiles don't allocate a
        // thousand copies of the same cube. Deliberately absent from `tests.rs`.
        .init_resource::<render::BlockAssets>()
        // An *observer* is a system that runs in response to a triggered event rather
        // than on a schedule. `SpawnLevel` is triggered from two very different places
        // (startup, and the restart key), and an observer lets both share one code
        // path without either needing to know about the other.
        .add_observer(level::spawn_level)
        // The other observer, and the only reason this app renders anything: it watches
        // for a `Block` component appearing and hangs a mesh on whatever it landed on.
        // Registering it here rather than inside `spawn_level` is what lets the headless
        // tests build the real level without an asset system — see `render`.
        .add_observer(render::add_block_visual)
        // `Startup` runs once, before the first `Update`.
        .add_systems(
            Startup,
            (
                camera::spawn_camera,
                ui::spawn_hud,
                level::request_first_level,
            ),
        )
        // `Update` runs once per rendered frame.
        //
        // `.chain()` is the important part: it forces these systems to run in exactly
        // this order. Without it Bevy would run them in parallel across threads, and
        // "resolve collisions" landing before "apply gravity" would be a coin flip
        // every frame. Ordering in a physics step is not optional, so we spell it out.
        //
        // Read this list top to bottom and you have the entire per-frame game logic:
        .add_systems(
            Update,
            (
                // 1. The clock. Being inside this `Playing`-gated chain is the whole
                //    reason the run timer freezes on the win screen — see `tick_timer`.
                gameplay::tick_timer,
                // 2. Intent: what does the player want to do?
                player::player_input,
                // 3. Forces: gravity pulls everything with a `Gravity` component down.
                physics::apply_gravity,
                // 4. Move the world: platforms shift, and anything standing on one
                //    rides along with it.
                physics::move_platforms,
                physics::carry_platform_riders,
                // 5. Move the movers, then push them back out of anything solid.
                physics::move_and_collide,
                // 6. React to the new positions: pick up coins, die on spikes, win.
                gameplay::animate_coins,
                gameplay::collect_coins,
                gameplay::check_hazards,
                gameplay::check_goal,
                // 7. Finally point the camera at wherever the player ended up. Doing
                //    this last is what keeps the camera from lagging a frame behind.
                camera::follow_player,
            )
                .chain()
                // ...and the whole chain is skipped unless we're actually playing.
                .run_if(in_state(GameState::Playing)),
        )
        // These two run in *every* state — the HUD should stay readable on the win
        // screen, and `R` has to work there too (that's how you play again).
        .add_systems(Update, (ui::update_hud, ui::restart_input))
        // `OnEnter` schedules fire once, during the frame the state changes.
        .add_systems(OnEnter(GameState::Won), ui::spawn_win_overlay)
        .run();
}
