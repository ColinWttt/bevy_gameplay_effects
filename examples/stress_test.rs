use std::time::Duration;

use bevy::prelude::*;
use bevy::diagnostic::{FrameTimeDiagnosticsPlugin, LogDiagnosticsPlugin};
use bevy::time::common_conditions::on_timer;
use bevy::window::PresentMode;
use bevy_stat_effects::{prelude::*, stats, StackingBehaviors};

// Unfortunately effect systems are single threaded due to borrow issues
// but performance is still good.

const ENTITIES_TO_SPAWN: usize = 150_000;

stats! (
    CharacterStats {
        Health,
        HealthRegen,
        Strength,
    }
);

/// Unfortunately we have to feed in the variants as an array
/// since rust can't infer them at compile time.
const VARIANTS: [CharacterStats; 3] = [
    CharacterStats::Health,
    CharacterStats::HealthRegen,
    CharacterStats::Strength
];

/// Some effects we can use
struct HealingEffect;
struct OnFireEffect;
struct DamageEffect;
struct DeathEffect;

fn main() {
    let mut app = App::new();

    let stacking_behaviors = StackingBehaviors::new()
        .stack::<OnFireEffect>(StackingPolicy::NoStackingResetDuration) 
        .stack::<HealingEffect>(StackingPolicy::MultipleEffects(2)); // Can stack up to 2 healing effects

    app.add_plugins((
        DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "No VSync".to_string(),
                resolution: (800., 600.).into(),
                present_mode: PresentMode::Immediate, // <- disables VSync
                ..default()
            }),
            ..default()
        }),
        LogDiagnosticsPlugin::default(),
        FrameTimeDiagnosticsPlugin::default(),
        StatEffectsPlugin::<CharacterStats>::new(stacking_behaviors),
    ));

    app.add_systems(Startup, spawn_entities);
    app.add_systems(Update, (
        do_some_effects
            .run_if(on_timer(Duration::from_millis(500))),
        check_deaths,
    ));

    app.run();
}

fn spawn_entities(mut commands: Commands) {
    let active_effects = ActiveEffects::new([
        GameplayEffect::new::<DeathEffect>(
            CharacterStats::Health,
            EffectMagnitude::Fixed(0.),
            EffectCalculation::LowerBound,
            EffectDuration::Persistent(None),
        ),
    ]);
    let stats = GameplayStats::new(
        |stat| {
            match stat {
                CharacterStats::Health => 100.,
                CharacterStats::HealthRegen => 1.,
                CharacterStats::Strength => 5.,
            }
        }, VARIANTS
    );

    commands.spawn_batch((0..ENTITIES_TO_SPAWN).map(
        move |_| {
            (
                stats.clone(),
                active_effects.clone(),
            )
        })
    );
}

fn do_some_effects(
    mut commands: Commands,
    entities: Query<Entity, With<ActiveEffects<CharacterStats>>>,
) {
    let damage_effect = GameplayEffect::new::<DamageEffect>(
        CharacterStats::Health,
        EffectMagnitude::LocalStat(CharacterStats::Strength, StatScalingParams{multiplier: -1.0, ..default()}),
        EffectCalculation::Additive,
        EffectDuration::Immediate,
    );

    for entity in entities {
        // Take some damage
        commands.trigger(AddEffect(AddEffectData::new(
            entity, damage_effect.clone()
        )));
    }
}

fn check_deaths(
    mut commands: Commands,
    mut events: EventReader<OnBoundsBreached<CharacterStats>>,
) {
    // Since all entities are receiving the same damage each frame they will all die
    // and fire these events at the same time.  This causes a big fps drop, down to 60 fps for me.
    // This is not a realistic in-game condition, but it shows that it can handle a good amount
    // of entities.

    // This will give 100 Health/s for 5 seconds
    let healing_effect = GameplayEffect::new::<HealingEffect>(
        CharacterStats::Health,
        EffectMagnitude::Fixed(100.0),
        EffectCalculation::Additive,
        EffectDuration::Continuous(Some(5.0.into())),
    );
    for event in events.read() {
        if event.0.stat == CharacterStats::Health && event.0.bound == EffectCalculation::LowerBound {
            // Oh no entity died, let's heal him!
            commands.trigger(AddEffect(AddEffectData::new(event.target_entity, healing_effect.clone())));
        }
    }
}