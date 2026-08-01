//! Headless tests for the game logic.
//!
//! Run with `cargo test`. No window opens, no GPU is touched, and each test finishes in
//! milliseconds — because a Bevy app is just an `App` you can build with a different set
//! of plugins. Swapping `DefaultPlugins` for `MinimalPlugins` strips out windowing,
//! rendering, and input while leaving the ECS, the schedules, and *all of our systems*
//! completely intact.
//!
//! Two tricks make the tests deterministic, and both are worth knowing:
//!
//! 1. **[`bevy::time::TimePlugin`] is disabled** and we advance [`Time`] by hand. With the
//!    real clock, `delta_secs()` would depend on how fast the test machine is, and a
//!    physics assertion like "the jump peaks at 107 units" would be flaky. Here every
//!    frame is exactly 1/60s, so a test that passes once passes always.
//! 2. **`ButtonInput<KeyCode>` is inserted manually** so we can synthesize key presses.
//!    Since [`bevy::input::InputPlugin`] isn't present, nothing clears `just_pressed`
//!    for us, so [`Harness::step`] does it at the end of each frame — a good reminder of
//!    what that plugin quietly does for you every frame in the real game.

use core::time::Duration;

use bevy::ecs::system::RunSystemOnce;
use bevy::prelude::*;

use crate::{
    GameState, TILE, camera, gameplay,
    gameplay::Progress,
    level::{self, LEVEL, PlayerSpawn, SpawnLevel, tile_center},
    physics::{self, GroundState, Velocity},
    player::{self, JUMP_SPEED, Player},
    ui,
};

/// One simulated frame: 60fps exactly.
const FRAME: Duration = Duration::from_nanos(16_666_667);

/// A headless game, plus helpers for poking at it.
struct Harness {
    app: App,
}

impl Harness {
    /// Build the game with the same systems `main` registers, minus anything visual.
    fn new() -> Self {
        let mut app = App::new();

        app.add_plugins(
            // `.build()` turns the plugin *group* into something we can edit before it's
            // added. See the module docs for why `TimePlugin` has to go.
            MinimalPlugins.build().disable::<bevy::time::TimePlugin>(),
        )
        // `MinimalPlugins` has no state machine, so add just that piece of what
        // `DefaultPlugins` would have brought in.
        .add_plugins(bevy::state::app::StatesPlugin)
        // Time and input would normally come from the plugins we just skipped.
        .insert_resource(Time::<()>::default())
        .insert_resource(ButtonInput::<KeyCode>::default())
        .init_state::<GameState>()
        .init_resource::<Progress>()
        .init_resource::<PlayerSpawn>()
        .init_resource::<level::LevelBounds>()
        .add_observer(level::spawn_level)
        // Deliberately the same chain, in the same order, as `main`. If the two ever
        // drift apart the tests stop testing the real game — which is a good argument
        // for factoring this list into a shared plugin once a project grows.
        .add_systems(
            Update,
            (
                gameplay::tick_timer,
                player::player_input,
                physics::apply_gravity,
                physics::move_platforms,
                physics::carry_platform_riders,
                physics::move_and_collide,
                gameplay::animate_coins,
                gameplay::collect_coins,
                gameplay::check_hazards,
                gameplay::check_goal,
            )
                .chain()
                .run_if(in_state(GameState::Playing)),
        );

        // Build the level immediately rather than waiting for a `Startup` schedule, so
        // tests can inspect and reposition the player before the first frame runs.
        app.world_mut().trigger(SpawnLevel);
        app.update();

        Self { app }
    }

    /// Advance the simulation by `frames` frames of exactly 1/60s.
    fn step(&mut self, frames: u32) {
        for _ in 0..frames {
            self.app
                .world_mut()
                .resource_mut::<Time>()
                .advance_by(FRAME);
            self.app.update();

            // Clear the one-frame-only flags after each frame, the way `InputPlugin`
            // does in the real game. Note that `clear` only drops `just_pressed` and
            // `just_released` — a key stays *held* until something releases it, so
            // `press` once followed by `step(60)` models holding a key for a second.
            self.app
                .world_mut()
                .resource_mut::<ButtonInput<KeyCode>>()
                .clear();
        }
    }

    fn press(&mut self, key: KeyCode) {
        self.app
            .world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(key);
    }

    fn release(&mut self, key: KeyCode) {
        self.app
            .world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .release(key);
    }

    /// Run a one-off closure as a system against the world.
    ///
    /// `run_system_once` is the escape hatch that makes ECS state easy to test: rather
    /// than fighting borrow-checker gymnastics to reach into the `World`, hand Bevy a
    /// closure with normal system parameters and let it do the fetching.
    fn read<Out: 'static, Marker>(&mut self, system: impl IntoSystem<(), Out, Marker>) -> Out {
        self.app
            .world_mut()
            .run_system_once(system)
            .expect("system ran")
    }

    fn player_position(&mut self) -> Vec2 {
        self.read(|player: Query<&Transform, With<Player>>| {
            player
                .single()
                .expect("player exists")
                .translation
                .truncate()
        })
    }

    fn on_ground(&mut self) -> bool {
        self.read(|player: Query<&GroundState, With<Player>>| {
            player.single().expect("player exists").on_ground
        })
    }

    /// Teleport the player and zero their velocity — the setup step for most tests.
    fn place_player(&mut self, position: Vec2) {
        self.read(
            move |mut player: Query<(&mut Transform, &mut Velocity), With<Player>>| {
                let (mut transform, mut velocity) = player.single_mut().expect("player exists");
                transform.translation.x = position.x;
                transform.translation.y = position.y;
                velocity.0 = Vec2::ZERO;
            },
        );
    }

    fn set_player_velocity(&mut self, velocity: Vec2) {
        self.read(move |mut player: Query<&mut Velocity, With<Player>>| {
            player.single_mut().expect("player exists").0 = velocity;
        });
    }

    fn progress(&self) -> &Progress {
        self.app.world().resource::<Progress>()
    }

    fn state(&self) -> GameState {
        *self.app.world().resource::<State<GameState>>().get()
    }
}

/// The world position of the first `character` in [`LEVEL`].
///
/// Tests that need to reach a particular feature look it up instead of hardcoding tile
/// coordinates. The README invites you to redesign the level by editing the ASCII art, so a
/// test that breaks when you move the goal two rows up is testing the wrong thing — it
/// should still be checking "touching the flag wins", wherever the flag happens to be.
fn find_tile(character: char) -> Vec2 {
    for (row, line) in LEVEL.iter().enumerate() {
        if let Some(column) = line.chars().position(|c| c == character) {
            return tile_center(column as f32, row as f32);
        }
    }
    panic!("LEVEL contains no {character:?}");
}

/// The ASCII art is load-bearing, so guard its invariants. These are the same asserts
/// `spawn_level` makes, but as a test they name the rule instead of just enforcing it.
#[test]
fn level_art_is_well_formed() {
    let width = LEVEL[0].len();
    for (row, line) in LEVEL.iter().enumerate() {
        assert_eq!(line.len(), width, "row {row} is a different length");
    }

    let spawns = LEVEL
        .iter()
        .flat_map(|row| row.chars())
        .filter(|c| *c == 'P')
        .count();
    assert_eq!(spawns, 1, "there must be exactly one player spawn");

    let goals = LEVEL
        .iter()
        .flat_map(|row| row.chars())
        .filter(|c| *c == 'F')
        .count();
    assert_eq!(goals, 1, "there must be exactly one goal");
}

#[test]
fn level_builds_with_the_expected_contents() {
    let mut harness = Harness::new();

    let coins = harness.progress().total_coins;
    assert!(coins > 0, "the level should contain coins");

    let solids = harness.read(|solids: Query<(), With<physics::Solid>>| solids.iter().count());
    let expected_solids = LEVEL
        .iter()
        .flat_map(|row| row.chars())
        .filter(|c| matches!(c, '#' | '='))
        .count()
        + level::MOVING_PLATFORMS.len();
    assert_eq!(solids, expected_solids);
}

#[test]
fn player_falls_and_lands_on_the_floor() {
    let mut harness = Harness::new();

    // Spawn sits just above the floor, so this is a very short drop.
    harness.step(30);

    assert!(harness.on_ground(), "player should have landed");

    // The floor's top surface is y = TILE (row 17 spans 0..32), and the player's
    // collider is 28 tall, so resting height is TILE + 14.
    let resting = harness.player_position().y;
    assert!(
        (resting - (TILE + 14.0)).abs() < 0.5,
        "expected to rest at {}, got {resting}",
        TILE + 14.0
    );

    // And once landed, they should stay put rather than sinking or jittering.
    harness.step(60);
    assert!((harness.player_position().y - resting).abs() < 0.01);
}

#[test]
fn holding_jump_clears_three_tiles() {
    let mut harness = Harness::new();
    harness.step(30);
    let ground_y = harness.player_position().y;

    // Press once and never release: the key stays held for the whole flight.
    harness.press(KeyCode::Space);

    let mut peak = ground_y;
    for _ in 0..60 {
        harness.step(1);
        peak = peak.max(harness.player_position().y);
    }

    // Peak height is JUMP_SPEED² / (2·|GRAVITY|). Assert against the formula rather than
    // a magic number, so retuning the constants doesn't turn into a failing test — it
    // stays a check that the physics matches the math.
    let expected = JUMP_SPEED * JUMP_SPEED / (2.0 * physics::GRAVITY.abs());
    let height = peak - ground_y;
    assert!(
        (height - expected).abs() < 6.0,
        "expected a jump of about {expected} units, got {height}"
    );
    assert!(
        height > 3.0 * TILE,
        "the level's 3-tile gaps need a jump taller than {} units",
        3.0 * TILE
    );
}

#[test]
fn tapping_jump_gives_a_shorter_hop() {
    let mut harness = Harness::new();
    harness.step(30);
    let ground_y = harness.player_position().y;

    // Hold for two frames, then let go: `JUMP_CUT_MULTIPLIER` should cut the arc short.
    harness.press(KeyCode::Space);
    harness.step(2);
    harness.release(KeyCode::Space);

    let mut peak = ground_y;
    for _ in 0..60 {
        harness.step(1);
        peak = peak.max(harness.player_position().y);
    }

    let full_jump = JUMP_SPEED * JUMP_SPEED / (2.0 * physics::GRAVITY.abs());
    let height = peak - ground_y;
    assert!(
        height < full_jump * 0.6,
        "a tap should be well short of a {full_jump}-unit full jump, got {height}"
    );
    assert!(
        height > 8.0,
        "a tap should still leave the ground, got {height}"
    );
}

#[test]
fn cannot_jump_out_of_thin_air() {
    let mut harness = Harness::new();

    // Drop the player into open space well above the floor, and give coyote time longer
    // than its 0.10s window to expire.
    harness.place_player(tile_center(2.0, 6.0));
    harness.step(20);

    let before = harness.player_position().y;
    harness.press(KeyCode::Space);
    harness.step(1);

    // Still falling: the jump was correctly refused.
    assert!(harness.player_position().y < before);
}

#[test]
fn running_into_a_wall_stops_horizontal_movement() {
    let mut harness = Harness::new();
    harness.step(30);

    // There's a one-tile-tall block at columns 7-8 of row 16, which is too tall to walk
    // up. Hold right long enough to reach it and press against it.
    harness.press(KeyCode::KeyD);
    harness.step(90);

    let position = harness.player_position();

    // Column 7's left edge is at 7 * TILE = 224; the player's half-width is 10.
    let wall_x = 7.0 * TILE - 10.0;
    assert!(
        position.x <= wall_x + 0.5,
        "player at {} should have been stopped by the wall at {wall_x}",
        position.x
    );
    assert!(position.x > 180.0, "player barely moved: {}", position.x);

    let velocity_x = harness
        .read(|player: Query<&Velocity, With<Player>>| player.single().expect("player exists").0.x);
    assert!(
        velocity_x.abs() < 1.0,
        "velocity should be zeroed against a wall, got {velocity_x}"
    );
}

#[test]
fn running_right_collects_the_first_coin() {
    let mut harness = Harness::new();
    harness.step(30);
    assert_eq!(harness.progress().coins, 0);

    // A coin sits at column 5, between the spawn and the wall at column 7.
    harness.press(KeyCode::KeyD);
    harness.step(90);

    assert_eq!(
        harness.progress().coins,
        1,
        "should have picked up one coin"
    );

    // And it should really be gone from the world, not just counted twice.
    let remaining = harness.read(|coins: Query<(), With<gameplay::Coin>>| coins.iter().count());
    assert_eq!(remaining as u32, harness.progress().total_coins - 1);
}

#[test]
fn falling_onto_spikes_costs_a_life_and_respawns() {
    let mut harness = Harness::new();
    let spawn = harness.player_position();

    // Row 17 has spikes at columns 25-26. Drop in from just above them.
    harness.place_player(tile_center(25.0, 15.0));
    harness.step(30);

    assert_eq!(harness.progress().deaths, 1);

    let position = harness.player_position();
    assert!(
        position.distance(spawn) < TILE,
        "expected a respawn near {spawn}, got {position}"
    );
}

#[test]
fn falling_into_the_pit_respawns() {
    let mut harness = Harness::new();
    let spawn = harness.player_position();

    // Columns 28-37 of row 17 are empty: a bottomless pit.
    harness.place_player(tile_center(32.0, 10.0));
    harness.step(90);

    assert_eq!(harness.progress().deaths, 1);
    assert!(harness.player_position().distance(spawn) < TILE);
    assert!(
        harness.on_ground(),
        "should have landed back on the spawn floor"
    );
}

#[test]
fn one_way_platforms_are_passable_from_below_and_solid_from_above() {
    let mut harness = Harness::new();

    // The one-way run at row 13, columns 17-20. Its top surface ends up at row 13's top
    // edge, which is 4 tiles above the floor.
    let platform_top = 5.0 * TILE;
    let column_x = tile_center(18.0, 13.0).x;

    // Start below it, moving up fast.
    harness.place_player(Vec2::new(column_x, platform_top - 2.5 * TILE));
    harness.set_player_velocity(Vec2::new(0.0, JUMP_SPEED));

    // Rise through it...
    harness.step(12);
    assert!(
        harness.player_position().y > platform_top,
        "should have passed up through the one-way platform"
    );

    // ...then fall back and land on top of it.
    harness.step(60);
    assert!(harness.on_ground(), "should have landed on the platform");
    let resting = harness.player_position().y;
    assert!(
        (resting - (platform_top + 14.0)).abs() < 1.5,
        "expected to rest at {}, got {resting}",
        platform_top + 14.0
    );
}

#[test]
fn moving_platforms_carry_the_player() {
    let mut harness = Harness::new();

    // The first spec is the horizontal platform crossing the pit. Stand on it at its
    // starting position and let go of the controls entirely.
    let spec = &level::MOVING_PLATFORMS[0];
    let platform_start = tile_center(spec.from.0, spec.from.1);
    harness.place_player(Vec2::new(platform_start.x, platform_start.y + TILE));
    harness.step(10);
    let landed_y = harness.player_position().y;

    // Check every frame, not just the end: a rider that gets ejected and lands back on
    // the platform would pass an end-state-only assertion.
    for frame in 0..60 {
        harness.step(1);
        assert!(
            harness.on_ground(),
            "fell off the platform on frame {frame} at {}",
            harness.player_position()
        );
        assert!(
            (harness.player_position().y - landed_y).abs() < 0.5,
            "drifted vertically on frame {frame}"
        );
    }

    let carried = harness.player_position().x - platform_start.x;
    assert!(
        carried > TILE,
        "the platform should have carried the player at least a tile, moved {carried}"
    );
}

/// Regression test: standing still on the *vertical* elevator used to eject the player
/// sideways. See the "skin width" comment in `physics::move_and_collide`.
#[test]
fn riding_the_vertical_platform_does_not_shove_the_player_sideways() {
    let mut harness = Harness::new();

    // `MOVING_PLATFORMS[1]` is the elevator up to the goal ledge.
    let spec = &level::MOVING_PLATFORMS[1];
    let platform_start = tile_center(spec.from.0, spec.from.1);

    // Drop onto it, dead centre, and never touch the controls again.
    harness.place_player(Vec2::new(platform_start.x, platform_start.y + TILE));
    harness.step(10);
    assert!(harness.on_ground(), "should have landed on the elevator");
    let landed_y = harness.player_position().y;

    // Ride for four seconds — long enough for a full trip up (192 units at 85 units/sec
    // is ~2.3s) *and* most of the way back down, so both directions get covered.
    let mut peak = landed_y;
    for frame in 0..240 {
        harness.step(1);
        let position = harness.player_position();
        peak = peak.max(position.y);

        assert!(
            harness.on_ground(),
            "fell off the elevator on frame {frame} at {position}"
        );
        assert!(
            (position.x - platform_start.x).abs() < 0.5,
            "drifted sideways to {} on frame {frame}, started at {}",
            position.x,
            platform_start.x
        );
    }

    // Check the peak rather than the final position: after 4 seconds the elevator has
    // turned around and is heading back down, so net displacement is near zero even
    // though the player has been carried the full height and back.
    let climbed = peak - landed_y;
    assert!(
        climbed > 4.0 * TILE,
        "the elevator should have carried the player most of its 6-tile run, got {climbed}"
    );
}

#[test]
fn the_run_timer_counts_real_seconds() {
    let mut harness = Harness::new();
    assert_eq!(harness.progress().time, 0.0, "should start at zero");

    // Start the clock, then 120 frames of exactly 1/60s each.
    harness.press(KeyCode::KeyD);
    harness.step(120);

    let elapsed = harness.progress().time;
    assert!(
        (elapsed - 2.0).abs() < 0.01,
        "120 frames at 1/60s should be 2 seconds, got {elapsed}"
    );
}

#[test]
fn the_run_timer_waits_for_the_first_input() {
    let mut harness = Harness::new();

    // Two seconds of the player sitting on the spawn platform doing nothing.
    harness.step(120);
    assert_eq!(
        harness.progress().time,
        0.0,
        "the clock should not run before the player has touched the controls"
    );
    assert!(!harness.progress().timer_started);

    harness.press(KeyCode::Space);
    harness.step(30);

    assert!(harness.progress().timer_started);
    let elapsed = harness.progress().time;
    assert!(
        (elapsed - 0.5).abs() < 0.01,
        "should have timed only the 30 frames since the first input, got {elapsed}"
    );
}

/// The restart key must not start the clock. It resets the timer *and* is a keypress, so if
/// it counted as input, every run would begin with time already on it — and how much would
/// depend on whether `restart_input` ran before or after `tick_timer` that frame.
#[test]
fn the_restart_key_does_not_start_the_clock() {
    let mut harness = Harness::new();

    harness.press(KeyCode::KeyR);
    harness.step(60);

    assert_eq!(harness.progress().time, 0.0);
    assert!(!harness.progress().timer_started);
}

#[test]
fn the_run_timer_freezes_on_the_win_screen() {
    let mut harness = Harness::new();
    harness.press(KeyCode::KeyD);
    harness.step(60);
    assert!(harness.progress().time > 0.5, "the clock should be running");

    harness.place_player(find_tile('F'));
    harness.step(2);
    assert_eq!(harness.state(), GameState::Won);

    // The timer lives inside the `Playing`-gated chain, so leaving `Playing` should stop
    // it dead — no explicit pause handling anywhere.
    let finish_time = harness.progress().time;
    harness.step(120);
    assert_eq!(
        harness.progress().time,
        finish_time,
        "the clock kept running after the level was finished"
    );
}

#[test]
fn restarting_resets_the_run_timer() {
    let mut harness = Harness::new();
    harness.press(KeyCode::KeyD);
    harness.step(90);
    assert!(harness.progress().time > 1.0);

    harness.release(KeyCode::KeyD);
    harness.app.world_mut().trigger(SpawnLevel);
    assert_eq!(
        harness.progress().time,
        0.0,
        "restart should zero the clock"
    );
    assert!(
        !harness.progress().timer_started,
        "restart should re-arm the clock"
    );

    // Nothing held, so the fresh run should stay parked at zero...
    harness.step(60);
    assert_eq!(
        harness.progress().time,
        0.0,
        "the new run started counting before any input"
    );

    // ...until the player moves again.
    harness.press(KeyCode::KeyD);
    harness.step(60);
    assert!((harness.progress().time - 1.0).abs() < 0.01);
}

/// Holding a direction through a restart should start the new run's clock immediately —
/// you clearly meant to keep playing, so there is nothing to wait for.
#[test]
fn restarting_while_holding_a_direction_starts_the_clock_at_once() {
    let mut harness = Harness::new();
    harness.press(KeyCode::KeyD);
    harness.step(30);

    harness.app.world_mut().trigger(SpawnLevel);
    assert_eq!(harness.progress().time, 0.0);

    harness.step(30);
    assert!(
        (harness.progress().time - 0.5).abs() < 0.01,
        "expected the held key to restart the clock, got {}",
        harness.progress().time
    );
}

#[test]
fn times_are_formatted_as_minutes_seconds_hundredths() {
    assert_eq!(ui::format_time(0.0), "0:00.00");
    assert_eq!(ui::format_time(1.5), "0:01.50");
    assert_eq!(ui::format_time(9.07), "0:09.07");
    assert_eq!(ui::format_time(59.99), "0:59.99");
    assert_eq!(ui::format_time(60.0), "1:00.00");
    assert_eq!(ui::format_time(125.25), "2:05.25");
    assert_eq!(ui::format_time(3600.0), "60:00.00");

    // The trap this function exists to avoid: a value that rounds up to a full minute
    // must roll over, not render as `0:60.00`.
    assert_eq!(ui::format_time(59.999), "1:00.00");

    // Defensive: a negative duration should never appear, but if one did it should not
    // produce a garbage string from an `as u64` cast of a negative float.
    assert_eq!(ui::format_time(-1.0), "0:00.00");
}

#[test]
fn touching_the_flag_wins() {
    let mut harness = Harness::new();
    assert_eq!(harness.state(), GameState::Playing);

    harness.place_player(find_tile('F'));
    harness.step(2);

    assert_eq!(harness.state(), GameState::Won);
}

#[test]
fn restarting_resets_progress_and_rebuilds_the_level() {
    let mut harness = Harness::new();

    // Make a mess: collect a coin, die, and win.
    harness.step(30);
    harness.press(KeyCode::KeyD);
    harness.step(90);
    harness.release(KeyCode::KeyD);
    harness.place_player(find_tile('F'));
    harness.step(2);

    let total_coins = harness.progress().total_coins;
    assert_eq!(harness.progress().coins, 1);
    assert_eq!(harness.state(), GameState::Won);

    harness.app.world_mut().trigger(SpawnLevel);
    harness.step(1);

    assert_eq!(harness.progress().coins, 0);
    assert_eq!(harness.progress().deaths, 0);
    assert_eq!(harness.progress().total_coins, total_coins);
    assert_eq!(harness.state(), GameState::Playing);

    // Exactly one player, not two — this is what the `LevelEntity` teardown buys us.
    let players = harness.read(|players: Query<(), With<Player>>| players.iter().count());
    assert_eq!(players, 1);

    let coins = harness.read(|coins: Query<(), With<gameplay::Coin>>| coins.iter().count());
    assert_eq!(coins as u32, total_coins);
}

#[test]
fn camera_stays_inside_the_level() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins.build().disable::<bevy::time::TimePlugin>())
        .insert_resource(Time::<()>::default())
        .init_resource::<level::LevelBounds>();

    // A camera and a player, with no rendering involved: `follow_player` only reads
    // `Transform` and `Projection`, so it tests perfectly well headless.
    let bounds = Rect::new(0.0, 0.0, 60.0 * TILE, 18.0 * TILE);
    app.insert_resource(level::LevelBounds(bounds));
    app.world_mut().spawn((
        Camera2d,
        Projection::Orthographic(OrthographicProjection {
            // `area` is normally maintained by the render pipeline, which isn't running
            // here, so set it directly to a plausible 16:9 view.
            area: Rect::new(-398.0, -224.0, 398.0, 224.0),
            ..OrthographicProjection::default_2d()
        }),
        Transform::default(),
    ));
    let player = app
        .world_mut()
        .spawn((Player::default(), Transform::default()))
        .id();
    app.add_systems(Update, camera::follow_player);

    // Park the player in the bottom-left corner and let the camera settle.
    for _ in 0..240 {
        app.world_mut().resource_mut::<Time>().advance_by(FRAME);
        app.update();
    }

    let camera_position = app
        .world_mut()
        .run_system_once(|camera: Query<&Transform, With<Camera2d>>| {
            camera
                .single()
                .expect("camera exists")
                .translation
                .truncate()
        })
        .unwrap();

    // The view is 796x448, so its center can never go below (398, 224).
    assert!(
        camera_position.x >= 398.0 - 1.0,
        "camera x leaked left: {camera_position}"
    );
    assert!(
        camera_position.y >= 224.0 - 1.0,
        "camera y leaked down: {camera_position}"
    );

    // Now send the player to the far corner and confirm it clamps on the other side too.
    app.world_mut()
        .entity_mut(player)
        .insert(Transform::from_xyz(10_000.0, 10_000.0, 0.0));
    for _ in 0..240 {
        app.world_mut().resource_mut::<Time>().advance_by(FRAME);
        app.update();
    }

    let camera_position = app
        .world_mut()
        .run_system_once(|camera: Query<&Transform, With<Camera2d>>| {
            camera
                .single()
                .expect("camera exists")
                .translation
                .truncate()
        })
        .unwrap();

    assert!(
        camera_position.x <= bounds.max.x - 398.0 + 1.0,
        "camera x leaked right"
    );
    assert!(
        camera_position.y <= bounds.max.y - 224.0 + 1.0,
        "camera y leaked up"
    );
}

#[test]
fn a_long_frame_does_not_drop_the_player_through_the_floor() {
    let mut harness = Harness::new();
    harness.step(30);
    assert_eq!(harness.progress().deaths, 0);
    let resting_y = harness.player_position().y;

    // Simulate one very long frame. This is not hypothetical: the first `Update` after
    // startup has to absorb window creation and GPU init, which on a real machine is
    // easily 100-300ms.
    harness
        .app
        .world_mut()
        .resource_mut::<Time>()
        .advance_by(Duration::from_millis(300));
    harness.app.update();

    assert_eq!(
        harness.progress().deaths,
        0,
        "a single long frame killed the player, who was standing still on the floor"
    );
    assert!(
        (harness.player_position().y - resting_y).abs() < 1.0,
        "player was displaced to {} from {resting_y}",
        harness.player_position().y
    );
}

/// The clamped timestep isn't a magic number — it exists to satisfy one inequality. Assert
/// it directly, so retuning `MAX_FALL_SPEED` or `MAX_TIMESTEP` fails here instead of
/// quietly reintroducing a player who falls through the world after a hitch.
#[test]
fn tunneling_is_impossible_by_construction() {
    let worst_case_travel = physics::MAX_FALL_SPEED * physics::MAX_TIMESTEP;
    assert!(
        worst_case_travel < TILE,
        "at terminal velocity a single clamped frame moves {worst_case_travel} units, \
         which is further than the {TILE}-unit floor is thick"
    );
}
