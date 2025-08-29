use std::time::Duration;

use bevy::image::DataFormat;
use bevy::prelude::*;
use bevy::diagnostic::{FrameTimeDiagnosticsPlugin, LogDiagnosticsPlugin};
use bevy::time::common_conditions::on_timer;
use bevy_stat_effects::{prelude::*, stats, StackingBehaviors};

// I was able to get 350k entities processing at 60fps.  YMMV
// Systems are single-threaded unfortunately
// This is spawning ~70k effects every half second, plus healing
// effects on entity death. It probably won't match real game conditions.
// It's just a stress test

// Unfortunately effect systems are single threaded

const ENTITIES_TO_SPAWN: usize = 350_000;

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
        DefaultPlugins,
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
        StatEffect::new::<DeathEffect>(
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
    // I think in a real game you're unlikely to be spawning new effects every frame
    // so let's do it on every 5th entity
    let mut n = 0;
    let damage_effect = StatEffect::new::<DamageEffect>(
        CharacterStats::Health,
        EffectMagnitude::Fixed(-10.0),
        EffectCalculation::Additive,
        EffectDuration::Immediate,
    );

    for entity in entities {
        n += 1;
        if n % 5 != 0 { continue; }
        // Take some damage
        commands.trigger(AddEffect(EffectMetadata::new(
            entity, damage_effect.clone()
        )));
    }
}

fn check_deaths(
    mut commands: Commands,
    mut events: EventReader<OnBoundsBreached<CharacterStats>>,
) {
    let healing_effect = StatEffect::new::<HealingEffect>(
        CharacterStats::Health,
        EffectMagnitude::Fixed(100.0),
        EffectCalculation::Additive,
        EffectDuration::Continuous(Some(5.0.into())),
    );
    for event in events.read() {
        if event.0.stat == CharacterStats::Health && event.0.bound == EffectCalculation::LowerBound {
            // Oh no entity died, let's heal him!
            commands.trigger(AddEffect(EffectMetadata::new(event.0.target_entity, healing_effect.clone())));
        }
    }
}