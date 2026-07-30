//! The rules: coins add up, spikes kill, the flag wins.
//!
//! Every system here follows the same shape — query the player, query the things, check
//! for overlap, react. That repetition is intentional. In an ECS you tend to end up with
//! many small systems that each answer one question, rather than one `update()` method
//! that does everything; the payoff is that adding a new interaction never means editing
//! an existing one.
//!
//! Note that none of these systems move anything (except `respawn`, which teleports).
//! They run *after* [`crate::physics::move_and_collide`] in the schedule and only look
//! at where things ended up.

use bevy::prelude::*;

use crate::{
    GameState,
    level::PlayerSpawn,
    physics::{Collider, Velocity, overlaps, step_seconds},
    player::{self, Player},
};

/// Below this world Y, you've fallen out of the level.
///
/// The level's floor is at y = 0, so anything meaningfully negative means the player
/// went down a pit. Waiting until -100 rather than 0 lets the fall read as a fall.
const FALL_OUT_Y: f32 = -100.0;

/// How fast coins bob, in radians per second.
const COIN_BOB_SPEED: f32 = 4.0;

/// How far coins bob, in world units.
const COIN_BOB_HEIGHT: f32 = 3.0;

/// The scoreboard. Reset wholesale on every level rebuild.
#[derive(Resource, Debug, Default)]
pub struct Progress {
    /// Coins picked up this attempt.
    pub coins: u32,
    /// How many there are in total, counted while the level is built.
    pub total_coins: u32,
    /// Deaths this attempt. Not punished — just displayed, so you can try to beat it.
    pub deaths: u32,
    /// Seconds of play this attempt — see [`tick_timer`].
    ///
    /// Accumulated by hand rather than read off `Time::elapsed_secs()`, because that
    /// clock starts when the *process* does and knows nothing about restarts. Summing
    /// deltas into a resource that `spawn_level` resets gives us a per-attempt timer for
    /// free, and it stops automatically whenever the gameplay systems stop running.
    pub time: f32,
    /// Whether [`time`](Self::time) has started advancing.
    ///
    /// `false` until the player's first control input, so the clock doesn't run while
    /// you're getting your bearings after a restart. Resets with the rest of `Progress`.
    pub timer_started: bool,
}

/// A collectible coin.
#[derive(Component, Debug)]
pub struct Coin {
    /// The Y the coin bobs around. Stored because [`animate_coins`] overwrites
    /// `Transform.y` outright each frame, and needs a fixed reference point to do that.
    /// Accumulating an offset into the transform instead would let float error drift.
    pub base_y: f32,
    /// Per-coin offset into the bob cycle, so a row of coins ripples instead of pulsing
    /// as one block.
    pub phase: f32,
}

/// Something that kills the player on contact.
#[derive(Component)]
pub struct Hazard;

/// The goal flag.
#[derive(Component)]
pub struct Goal;

/// Advance the run timer, once the player has actually started playing.
///
/// Two things worth noticing about how little this has to do:
///
/// * **Stopping is free.** This system sits inside the `run_if(in_state(Playing))` chain in
///   `main`, so it simply doesn't run on the win screen. No `paused` flag, no `if state ==
///   ...`, nothing to keep in sync — the timer freezes as a consequence of how the schedule
///   is already organized.
/// * **Resetting is free.** `spawn_level` does `*progress = Progress::default()`, which
///   zeroes the clock and re-arms it without knowing a timer exists.
///
/// Starting is the only part that needs real logic, and the subtlety is *which* keys count.
/// Only the movement and jump bindings do — deliberately not `R`. Restart both resets this
/// flag and is itself a keypress, so counting every key would mean each new run began with
/// time already on the clock, in a way that depends on whether `restart_input` happened to
/// run before or after this system.
pub fn tick_timer(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    mut progress: ResMut<Progress>,
) {
    if !progress.timer_started {
        // `any_pressed`, not `any_just_pressed`: if you restart while still holding a
        // direction, you meant to keep running, so the clock should start immediately.
        let touching_the_controls = keys.any_pressed(player::LEFT_KEYS)
            || keys.any_pressed(player::RIGHT_KEYS)
            || keys.any_pressed(player::JUMP_KEYS);

        if !touching_the_controls {
            return;
        }
        progress.timer_started = true;
    }

    progress.time += step_seconds(&time);
}

/// Bob the coins up and down.
///
/// Pure decoration, and it's worth noticing how little it costs: one component field,
/// one `sin`, no state machine, no animation system. Small continuous motion is what
/// makes a screen of rectangles feel like a game rather than a diagram.
pub fn animate_coins(time: Res<Time>, mut coins: Query<(&mut Transform, &Coin)>) {
    for (mut transform, coin) in &mut coins {
        let bob = (time.elapsed_secs() * COIN_BOB_SPEED + coin.phase).sin();
        transform.translation.y = coin.base_y + bob * COIN_BOB_HEIGHT;
    }
}

/// Despawn coins the player is touching, and count them.
pub fn collect_coins(
    mut commands: Commands,
    mut progress: ResMut<Progress>,
    player: Query<(&Transform, &Collider), With<Player>>,
    coins: Query<(Entity, &Transform, &Collider), With<Coin>>,
) {
    // `single` returns a `Result`, because "exactly one match" is a claim that can fail
    // — there's no player during the frame the level is being rebuilt. Bailing out with
    // `let ... else` is the normal way to handle that; it just means "not this frame".
    let Ok((player_transform, player_collider)) = player.single() else {
        return;
    };
    let player_center = player_transform.translation.truncate();

    for (coin_entity, coin_transform, coin_collider) in &coins {
        if overlaps(
            player_center,
            player_collider,
            coin_transform.translation.truncate(),
            coin_collider,
        ) {
            // Queued, not immediate — `Commands` records structural changes and Bevy
            // applies them once the schedule reaches a safe point. That's why we can
            // despawn while still iterating the query we're despawning from.
            commands.entity(coin_entity).despawn();
            progress.coins += 1;
        }
    }
}

/// Send the player back to the start if they touch a hazard or fall out of the world.
pub fn check_hazards(
    mut progress: ResMut<Progress>,
    spawn_point: Res<PlayerSpawn>,
    mut player: Query<(&mut Transform, &mut Velocity, &Collider), With<Player>>,
    // `Without<Player>` again: this system holds `&mut Transform` on the player, so it
    // has to prove to Bevy that the hazards can't possibly be the same entities.
    hazards: Query<(&Transform, &Collider), (With<Hazard>, Without<Player>)>,
) {
    let Ok((mut player_transform, mut velocity, player_collider)) = player.single_mut() else {
        return;
    };

    let player_center = player_transform.translation.truncate();

    let hit_hazard = hazards.iter().any(|(hazard_transform, hazard_collider)| {
        overlaps(
            player_center,
            player_collider,
            hazard_transform.translation.truncate(),
            hazard_collider,
        )
    });

    let fell_out = player_center.y < FALL_OUT_Y;

    if hit_hazard || fell_out {
        player_transform.translation.x = spawn_point.0.x;
        player_transform.translation.y = spawn_point.0.y;
        // Zeroing velocity matters: respawning mid-fall while still carrying -900 on Y
        // would drop you straight through the spawn platform before collision resolution
        // gets a chance to catch you.
        velocity.0 = Vec2::ZERO;
        progress.deaths += 1;
    }
}

/// Switch to [`GameState::Won`] when the player reaches the flag.
pub fn check_goal(
    mut next_state: ResMut<NextState<GameState>>,
    player: Query<(&Transform, &Collider), With<Player>>,
    goals: Query<(&Transform, &Collider), (With<Goal>, Without<Player>)>,
) {
    let Ok((player_transform, player_collider)) = player.single() else {
        return;
    };
    let player_center = player_transform.translation.truncate();

    for (goal_transform, goal_collider) in &goals {
        if overlaps(
            player_center,
            player_collider,
            goal_transform.translation.truncate(),
            goal_collider,
        ) {
            // Setting `NextState` doesn't take effect immediately — Bevy applies the
            // transition (and runs any `OnEnter` systems) between schedules. So this
            // frame finishes normally and the overlay appears on the next one.
            next_state.set(GameState::Won);
            return;
        }
    }
}
