//! The player: keyboard input, and the "game feel" constants that turn it into motion.
//!
//! There is exactly one system in this file, and all it does is write to `Velocity`.
//! It never touches `Transform`. Moving and collision are [`crate::physics`]'s job.
//! That split is worth internalizing: input decides *intent*, physics decides *outcome*.
//!
//! ## Why so many constants?
//!
//! Because platformer feel is entirely in the numbers, and hiding them inside the
//! system body makes them impossible to find and tune. Every one of these is safe to
//! change — go break something and see what it does.
//!
//! Three of them are worth understanding properly, because they're the difference
//! between a platformer that feels tight and one that feels broken:
//!
//! * [`COYOTE_TIME`] — you can still jump for a moment *after* walking off a ledge.
//! * [`JUMP_BUFFER`] — pressing jump slightly *before* landing still jumps on landing.
//! * [`JUMP_CUT_MULTIPLIER`] — a tap gives a short hop, holding gives a full jump.
//!
//! The first two exist because human reaction time is real. Players press jump a frame
//! or two off constantly, and a game that answers "no" is a game they call unresponsive
//! without ever being able to say why. Try setting both to `0.0` and playing a while —
//! it's the most instructive experiment in this repo.

use bevy::prelude::*;

use crate::physics::{GroundState, Velocity, step_seconds};

/// Top horizontal running speed, in world units per second.
pub const RUN_SPEED: f32 = 260.0;

/// How fast we reach [`RUN_SPEED`] while on the ground.
///
/// Acceleration rather than snapping straight to top speed. `2200` is high enough to
/// still feel immediate (about 0.12s to full speed) while adding a little weight.
pub const GROUND_ACCELERATION: f32 = 2200.0;

/// How fast we slow to a stop when no key is held, on the ground.
pub const GROUND_FRICTION: f32 = 2200.0;

/// Air control. Lower than the ground value, so a jump commits you somewhat.
pub const AIR_ACCELERATION: f32 = 1400.0;

/// Air drag. Deliberately weak — a jump should mostly preserve its momentum.
pub const AIR_FRICTION: f32 = 400.0;

/// Upward velocity applied at the moment of a jump.
///
/// Peak height is `JUMP_SPEED² / (2 · |GRAVITY|)` ≈ 107 units ≈ 3.3 tiles, which is
/// what makes the level's 3-tile gaps between platforms clearable but not trivial.
/// Change [`crate::physics::GRAVITY`] and you change the level's difficulty.
pub const JUMP_SPEED: f32 = 620.0;

/// Grace period after leaving the ground during which a jump still works.
///
/// Named after the cartoon coyote who keeps running past the cliff edge.
pub const COYOTE_TIME: f32 = 0.10;

/// How long a jump press is remembered while airborne, to fire on landing.
pub const JUMP_BUFFER: f32 = 0.12;

/// Upward velocity is multiplied by this when the jump key is released early.
///
/// This is what gives variable jump height: release at the start of the rise for a
/// small hop, hold through it for the full arc. One line, enormous feel difference.
pub const JUMP_CUT_MULTIPLIER: f32 = 0.45;

/// Keys that move the player left.
///
/// These three arrays are `pub` because [`crate::gameplay::tick_timer`] also needs to know
/// what counts as a control key — the run clock starts on the player's first input. Sharing
/// the constants means the two can't drift: rebind a key here and the timer follows.
///
/// `KeyCode` is a *physical* key position, so `KeyA` is the same key on QWERTY and AZERTY,
/// which is what you want for WASD movement.
pub const LEFT_KEYS: [KeyCode; 2] = [KeyCode::ArrowLeft, KeyCode::KeyA];

/// Keys that move the player right. See [`LEFT_KEYS`].
pub const RIGHT_KEYS: [KeyCode; 2] = [KeyCode::ArrowRight, KeyCode::KeyD];

/// Keys that jump. See [`LEFT_KEYS`].
pub const JUMP_KEYS: [KeyCode; 3] = [KeyCode::Space, KeyCode::ArrowUp, KeyCode::KeyW];

/// Marks the player entity, and stores the two input-forgiveness timers.
///
/// These live on the component rather than in a resource because they're *per-entity*
/// state. If you added a second player, they'd each need their own — and with this
/// layout you'd get that for free.
#[derive(Component, Debug, Default)]
pub struct Player {
    /// Seconds of coyote time left. Refilled to [`COYOTE_TIME`] while grounded.
    pub coyote_timer: f32,
    /// Seconds a buffered jump press stays valid. Set to [`JUMP_BUFFER`] on press.
    pub jump_buffer: f32,
    /// Which way the player last moved. `false` is right, which is also the direction
    /// the level runs, so `Default` starts you facing the way you're going.
    ///
    /// Nothing draws this yet — a flat-colored box looks the same mirrored, which was
    /// equally true of the `Sprite::flip_x` this replaced. It lives here anyway because
    /// facing is *player state*, not a rendering detail, and it stops becoming
    /// cosmetic the moment the camera swings around behind the player.
    pub facing_left: bool,
}

/// Read the keyboard and write the player's velocity.
pub fn player_input(
    time: Res<Time>,
    // `ButtonInput<KeyCode>` is a resource Bevy refreshes each frame.
    keys: Res<ButtonInput<KeyCode>>,
    // Note there is nothing render-related in this query. That's deliberate: it used to
    // ask for `&mut Sprite` in order to mirror the player, and a query for a component
    // that no longer exists matches *nothing* — silently. The game still built, still
    // ran, and simply ignored the keyboard. Keeping input free of rendering components
    // means swapping the renderer can't quietly disconnect the controls again.
    mut players: Query<(&mut Velocity, &mut Player, &GroundState)>,
) {
    let dt = step_seconds(&time);

    for (mut velocity, mut player, ground) in &mut players {
        // ---- Horizontal ---------------------------------------------------------

        // Offer both arrows and WASD. `any_pressed` is true if *any* of them is down.
        let left = keys.any_pressed(LEFT_KEYS);
        let right = keys.any_pressed(RIGHT_KEYS);

        // -1, 0, or +1. Holding both directions cancels out, which is the behavior
        // players expect (and stops a stuck key from locking you into a run).
        let direction = f32::from(right) - f32::from(left);

        let (acceleration, friction) = if ground.on_ground {
            (GROUND_ACCELERATION, GROUND_FRICTION)
        } else {
            (AIR_ACCELERATION, AIR_FRICTION)
        };

        if direction != 0.0 {
            velocity.0.x = move_toward(velocity.0.x, direction * RUN_SPEED, acceleration * dt);
            // Remember which way we're going. Recorded, not drawn — see `facing_left`.
            player.facing_left = direction < 0.0;
        } else {
            velocity.0.x = move_toward(velocity.0.x, 0.0, friction * dt);
        }

        // ---- Timers -------------------------------------------------------------

        if ground.on_ground {
            player.coyote_timer = COYOTE_TIME;
        } else {
            // `.max(0.0)` keeps these from drifting to large negative values over a
            // long fall. The comparisons below only care about `> 0.0`, but clamping
            // keeps the values meaningful if you ever print them while debugging.
            player.coyote_timer = (player.coyote_timer - dt).max(0.0);
        }

        if keys.any_just_pressed(JUMP_KEYS) {
            player.jump_buffer = JUMP_BUFFER;
        } else {
            player.jump_buffer = (player.jump_buffer - dt).max(0.0);
        }

        // ---- Jump ---------------------------------------------------------------

        // Note the symmetry: a jump happens when a *recent press* meets *recent
        // ground*. Neither has to be happening this exact frame. That single condition
        // is what both coyote time and jump buffering come down to.
        if player.jump_buffer > 0.0 && player.coyote_timer > 0.0 {
            velocity.0.y = JUMP_SPEED;
            // Spend both timers, or we'd re-trigger every frame for the next 100ms and
            // the player would rocket upward.
            player.jump_buffer = 0.0;
            player.coyote_timer = 0.0;
        }

        // Variable jump height. `just_released` fires on exactly one frame, so this
        // applies the cut once instead of decaying the velocity every frame while the
        // key is up. Only cut while still rising — cutting a fall does nothing useful.
        if keys.any_just_released(JUMP_KEYS) && velocity.0.y > 0.0 {
            velocity.0.y *= JUMP_CUT_MULTIPLIER;
        }
    }
}

/// Step `current` toward `target` by at most `max_delta`, without overshooting.
///
/// The workhorse of frame-rate-independent acceleration: because `max_delta` is always
/// passed in as `rate * delta_time`, the result is the same whether the game is running
/// at 60fps or 144fps. Multiplying velocity by a constant each frame — a tempting
/// one-liner — does *not* have that property, and makes the game feel different on
/// different machines.
fn move_toward(current: f32, target: f32, max_delta: f32) -> f32 {
    if (target - current).abs() <= max_delta {
        target
    } else {
        current + (target - current).signum() * max_delta
    }
}
