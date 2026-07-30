//! The HUD and the win overlay.
//!
//! Bevy's UI is a separate world from the sprites: it's a flexbox layout system built on
//! the [`Node`] component, positioned in screen space, and completely unaffected by the
//! camera's `Transform`. That's why the HUD stays in the corner while the world scrolls
//! underneath it.
//!
//! Text needs no font asset here because Bevy ships a built-in default font (the
//! `default_font` feature, on by default). Perfectly fine for a HUD; load a real font
//! through the `AssetServer` when you want to control how it looks.

use bevy::prelude::*;

use crate::{GameState, gameplay::Progress, level::SpawnLevel};

const COLOR_TEXT: Color = Color::srgb(0.92, 0.94, 0.98);
const COLOR_TEXT_DIM: Color = Color::srgb(0.62, 0.66, 0.76);
const COLOR_WIN: Color = Color::srgb(0.40, 0.92, 0.58);

/// Marks the text node that [`update_hud`] rewrites each frame.
///
/// A marker component is how you address "that one specific entity" in an ECS. The
/// alternative — stashing the `Entity` id in a resource — works, but then every reader
/// needs the resource, and the id can go stale.
#[derive(Component)]
pub struct HudText;

/// Build the HUD once, at startup.
///
/// Like the camera, this deliberately lacks a `LevelEntity` marker, so it survives level
/// restarts.
pub fn spawn_hud(mut commands: Commands) {
    commands.spawn((
        // A `Node` with no parent is a root UI node. `position_type: Absolute` takes it
        // out of the layout flow so we can pin it to a corner by hand.
        Node {
            position_type: PositionType::Absolute,
            top: px(12),
            left: px(16),
            flex_direction: FlexDirection::Column,
            row_gap: px(4),
            ..default()
        },
        children![
            (
                // `Text` on a UI node renders in screen space. (The world-space
                // equivalent, for text that scrolls with the level, is `Text2d`.)
                Text::new(""),
                TextFont {
                    font_size: FontSize::Px(22.0),
                    ..default()
                },
                TextColor(COLOR_TEXT),
                HudText,
            ),
            (
                Text::new("move: A/D or arrows    jump: space/W/up    restart: R"),
                TextFont {
                    font_size: FontSize::Px(15.0),
                    ..default()
                },
                TextColor(COLOR_TEXT_DIM),
            )
        ],
    ));
}

/// Rewrite the HUD line from the [`Progress`] resource.
///
/// Runs every frame in every state. It could be gated on `resource_changed::<Progress>`
/// as an optimization, but formatting one short string per frame is free, and the simple
/// version is one less thing to get subtly wrong.
pub fn update_hud(progress: Res<Progress>, mut hud: Query<&mut Text, With<HudText>>) {
    let Ok(mut text) = hud.single_mut() else {
        return;
    };

    // `Text` is a newtype around `String`, so this is just a string assignment. Writing
    // through the `Mut` guard is also what flags the entity as changed, which is how
    // Bevy knows to re-layout and re-render the glyphs.
    text.0 = format!(
        "{}    coins {}/{}    deaths {}",
        format_time(progress.time),
        progress.coins,
        progress.total_coins,
        progress.deaths
    );
}

/// Format a duration in seconds as `M:SS.CC`.
///
/// Done in integer centiseconds rather than with float formatting, because the obvious
/// `format!("{}:{:05.2}", mins, secs)` has a rounding trap: 59.999 seconds is 0 minutes
/// with a remainder that rounds to `60.00`, so the HUD would flash `0:60.00` on the way to
/// `1:00.00`. Rounding once, up front, into a single integer makes that impossible.
pub fn format_time(seconds: f32) -> String {
    let centiseconds = (seconds.max(0.0) * 100.0).round() as u64;
    let minutes = centiseconds / 6000;
    let whole_seconds = centiseconds % 6000 / 100;
    let hundredths = centiseconds % 100;
    format!("{minutes}:{whole_seconds:02}.{hundredths:02}")
}

/// Rebuild the level when `R` is pressed.
///
/// Registered outside the `Playing`-gated system chain, so it also works on the win
/// screen — which is how you play again.
pub fn restart_input(keys: Res<ButtonInput<KeyCode>>, mut commands: Commands) {
    if keys.just_pressed(KeyCode::KeyR) {
        // All this system knows is that the player asked for a restart. What that
        // actually involves lives in `level::spawn_level`, and neither side has to know
        // about the other.
        commands.trigger(SpawnLevel);
    }
}

/// Show the "you win" panel. Runs once, on entering [`GameState::Won`].
pub fn spawn_win_overlay(mut commands: Commands, progress: Res<Progress>) {
    let summary = if progress.coins == progress.total_coins {
        format!("every coin, {} deaths", progress.deaths)
    } else {
        format!(
            "{}/{} coins, {} deaths",
            progress.coins, progress.total_coins, progress.deaths
        )
    };

    commands.spawn((
        Node {
            // Fill the screen and center the panel, the flexbox way.
            width: percent(100),
            height: percent(100),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            ..default()
        },
        // Dim the game behind the panel. The last component of `srgba` is alpha.
        BackgroundColor(Color::srgba(0.04, 0.05, 0.09, 0.75)),
        // The magic bit: `DespawnOnExit` deletes this entity (and its children) when we
        // leave `Won`. Since restarting always transitions back to `Playing`, the overlay
        // cleans itself up and no system ever has to remember to remove it.
        DespawnOnExit(GameState::Won),
        children![(
            Node {
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                row_gap: px(10),
                padding: UiRect::all(px(28)),
                // Rounded corners are a field on `Node`, not a separate component.
                border_radius: BorderRadius::all(px(10)),
                ..default()
            },
            BackgroundColor(Color::srgb(0.10, 0.12, 0.18)),
            children![
                (
                    Text::new("you made it"),
                    TextFont {
                        font_size: FontSize::Px(46.0),
                        ..default()
                    },
                    TextColor(COLOR_WIN),
                ),
                (
                    // The final time, big, because it's the number you'll want to beat.
                    // `tick_timer` stopped running the moment we left `Playing`, so this
                    // is frozen at whatever it read when the flag was touched.
                    Text::new(format_time(progress.time)),
                    TextFont {
                        font_size: FontSize::Px(30.0),
                        ..default()
                    },
                    TextColor(COLOR_TEXT),
                ),
                (
                    Text::new(summary),
                    TextFont {
                        font_size: FontSize::Px(20.0),
                        ..default()
                    },
                    // Dimmer than the time above it, so the eye lands on the time first.
                    TextColor(COLOR_TEXT_DIM),
                ),
                (
                    Text::new("press R to play again"),
                    TextFont {
                        font_size: FontSize::Px(16.0),
                        ..default()
                    },
                    TextColor(COLOR_TEXT_DIM),
                )
            ],
        )],
    ));
}
