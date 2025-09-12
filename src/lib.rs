use std::{any::TypeId, marker::PhantomData};
use bevy::{platform::collections::HashMap, prelude::*};
use crate::{effects::{add_effect, process_active_effects, remove_effect}, prelude::*};

mod gameplay_stats;
mod effects;
mod timing;
mod calculation;
mod events;
mod enum_macro;

pub mod prelude {
    pub use crate::{
        stats,
        StatEffectsPlugin,
        StatEffectsSystemSet,
        gameplay_stats::{GameplayStat, GameplayStats, StatTrait},
        effects::{StatEffect, ActiveEffects},
        timing::EffectDuration,
        calculation::{EffectCalculation, StackingPolicy, EffectMagnitude, StatScalingParams},
        events::{EffectMetadata, EffectTypeMetadata, AddEffect, RemoveEffect, OnEffectAdded,
            OnEffectRemoved, OnBoundsBreached, OnRepeatingEffectTriggered, BoundsBreachedMetadata},
    };
}

pub struct StatEffectsPlugin<T: StatTrait>(StackingBehaviors, PhantomData<T>);

impl<T: StatTrait> Default for StatEffectsPlugin<T> {
    fn default() -> Self {
        Self::new(StackingBehaviors::default())
    }
}

impl<T: StatTrait> StatEffectsPlugin<T> {
    pub fn new(stacking: StackingBehaviors) -> Self {
        Self(stacking, PhantomData)
    }
}

#[derive(Resource, Default, Clone)]
pub struct StackingBehaviors(HashMap<TypeId, StackingPolicy>);

impl StackingBehaviors {
    pub fn new() -> Self {
        Self(HashMap::<TypeId, StackingPolicy>::new())
    }

    pub fn stack<T: Send + Sync + 'static>(mut self, policy: StackingPolicy) -> Self {
        self.0.insert(TypeId::of::<T>(), policy);
        self
    }
}


#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct StatEffectsSystemSet;

impl<T: StatTrait> Plugin for StatEffectsPlugin<T> {
    fn build(&self, app: &mut App) {
        app.add_event::<OnEffectAdded>();
        app.add_event::<OnEffectRemoved>();
        app.add_event::<OnRepeatingEffectTriggered>();
        app.add_event::<OnBoundsBreached<T>>();
        app.add_observer(add_effect::<T>);
        app.add_observer(remove_effect::<T>);
        app.add_systems(Update, process_active_effects::<T>.in_set(StatEffectsSystemSet));
        app.insert_resource(self.0.clone());
    }
}


#[cfg(test)]
mod tests {
    use std::time::Duration;
    use super::*;
    pub(crate) use bevy::{prelude::*, time::TimePlugin};
    pub(crate) use crate::prelude::*;

    stats!(
        MyStats {
            Health,
            HealthRegen,
            HealthMax,
            Strength,
        }
    );

    pub(crate) fn setup_app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins.build().disable::<TimePlugin>());
        app.world_mut().insert_resource::<Time>(Time::default());
        app.add_plugins(StatEffectsPlugin::<MyStats>::default());
        app
    }

    fn setup_entity<'a>(app: &mut App) -> (Entity, QueryState<(Entity, &'a GameplayStats<MyStats>, &'a ActiveEffects<MyStats>)>) {
        const VARIANTS: [MyStats; 4] = [MyStats::Health, MyStats::HealthRegen, MyStats::HealthMax, MyStats::Strength];
        let stats_component = GameplayStats::<MyStats>::new(
            |stat| {
                match stat {
                    MyStats::Health => { 100.0 },
                    MyStats::HealthRegen => { 5.0 },
                    MyStats::HealthMax => { 100.0 },
                    MyStats::Strength => { 10.0 },
                }
            },
            VARIANTS
        );
        let active_effects = ActiveEffects::<MyStats>::new(std::iter::empty());
        let entity = app.world_mut().spawn((
            stats_component,
            active_effects,
        )).id();
        let query = app.world_mut()
            .query::<(Entity, &GameplayStats<MyStats>, &ActiveEffects<MyStats>)>();
        app.update();
        (entity, query)
    }
    
    #[test] 
    fn test_lower_bound() {
        let mut app = setup_app();
        let (entity, mut query) = setup_entity(&mut app);

        struct DeathEffect;
        struct DamageEffect;
        app.world_mut().trigger(AddEffect(EffectMetadata::new(
            entity, 
            StatEffect::new::<DeathEffect>(
                MyStats::Health,
                EffectMagnitude::Fixed(0.),
                EffectCalculation::LowerBound,
                EffectDuration::Persistent(None),
            )
        )));
        app.world_mut().trigger(AddEffect(EffectMetadata::new(
            entity, 
            StatEffect::new::<DamageEffect>(
                MyStats::Health,
                EffectMagnitude::Fixed(-200.),
                EffectCalculation::Additive,
                EffectDuration::Immediate,
            )
        )));
        let (_, stats, _) = query.iter(app.world_mut()).next().unwrap();
        let health = stats.get(MyStats::Health).current_value;
        assert_eq!(health, 0.);

        let events = app.world_mut().resource_mut::<Events<OnBoundsBreached<MyStats>>>();
        let mut cursor = events.get_cursor();
        let mut events = cursor.read(&events);
        assert_eq!(events.len(), 1);
        
        let event = events.next().unwrap();
        assert!(matches!(event.bound, EffectCalculation::LowerBound));
        assert_eq!(event.target_entity, entity);
        assert_eq!(event.stat, MyStats::Health);
    }

    #[test] 
    fn test_upper_bound() {
        let mut app = setup_app();
        let (entity, mut query) = setup_entity(&mut app);

        struct MaxHealthEffect;
        struct HealthBuff;
        app.world_mut().trigger(AddEffect(EffectMetadata::new(
            entity, 
            StatEffect::new::<MaxHealthEffect>(
                MyStats::Health,
                EffectMagnitude::Fixed(150.),
                EffectCalculation::UpperBound,
                EffectDuration::Persistent(None),
            )
        )));
        app.world_mut().trigger(AddEffect(EffectMetadata::new(
            entity, 
            StatEffect::new::<HealthBuff>(
                MyStats::Health,
                EffectMagnitude::Fixed(200.),
                EffectCalculation::Additive,
                EffectDuration::Immediate,
            )
        )));
        let (_, stats, _) = query.iter(app.world_mut()).next().unwrap();
        let health = stats.get(MyStats::Health).current_value;
        assert_eq!(health, 150.);

        let events = app.world_mut().resource_mut::<Events<OnBoundsBreached<MyStats>>>();
        let mut cursor = events.get_cursor();
        let mut events = cursor.read(&events);
        assert_eq!(events.len(), 1);
        
        let event = events.next().unwrap();
        assert!(matches!(event.bound, EffectCalculation::UpperBound));
        assert_eq!(event.target_entity, entity);
        assert_eq!(event.stat, MyStats::Health);
    }

    #[test] 
    fn test_set_value() {
        let mut app = setup_app();
        let (entity, mut query) = setup_entity(&mut app);

        struct MaxHealthEffect;
        struct SetHealth;

        app.world_mut().trigger(AddEffect(EffectMetadata::new(
            entity, 
            StatEffect::new::<MaxHealthEffect>(
                MyStats::Health,
                EffectMagnitude::LocalStat(MyStats::HealthMax, StatScalingParams::default()),
                EffectCalculation::UpperBound,
                EffectDuration::Persistent(None),
            )
        )));

        // Try to set past max health
        app.world_mut().trigger(AddEffect(EffectMetadata::new(
            entity, 
            StatEffect::new::<SetHealth>(
                MyStats::Health,
                EffectMagnitude::Fixed(200.),
                EffectCalculation::SetValue,
                EffectDuration::Immediate,
            )
        )));
        let (_, stats, _) = query.iter(app.world_mut()).next().unwrap();
        let health = stats.get(MyStats::Health).current_value;
        assert_eq!(health, 100.);

        app.world_mut().trigger(AddEffect(EffectMetadata::new(
            entity, 
            StatEffect::new::<SetHealth>(
                MyStats::Health,
                EffectMagnitude::Fixed(50.),
                EffectCalculation::SetValue,
                EffectDuration::Immediate,
            )
        )));
        let (_, stats, _) = query.iter(app.world_mut()).next().unwrap();
        let health = stats.get(MyStats::Health).current_value;
        assert_eq!(health, 50.);

        app.world_mut().trigger(AddEffect(EffectMetadata::new(
            entity, 
            StatEffect::new::<SetHealth>(
                MyStats::Health,
                EffectMagnitude::LocalStat(MyStats::HealthMax, StatScalingParams::default()),
                EffectCalculation::SetValue,
                EffectDuration::Immediate,
            )
        )));
        let (_, stats, _) = query.iter(app.world_mut()).next().unwrap();
        let health = stats.get(MyStats::Health).current_value;
        assert_eq!(health, 100.);
    }

    #[test] 
    fn test_periodic_effect() {
        let mut app = setup_app();
        let (entity, mut query) = setup_entity(&mut app);

        struct HealthRegen;
        let scaling = StatScalingParams {
            multiplier: 2.0,
            ..default()
        };
        app.world_mut().trigger(AddEffect(EffectMetadata::new(
            entity, 
            StatEffect::new::<HealthRegen>(
                MyStats::Health,
                EffectMagnitude::LocalStat(MyStats::HealthRegen, scaling),
                EffectCalculation::Additive,
                EffectDuration::Repeating(1.0.into(), Some(10.0.into())),
            )
        )));
        let (_, stats, _) = query.iter(app.world_mut()).next().unwrap();
        let health = stats.get(MyStats::Health).current_value;
        assert_eq!(health, 100.);
        
        for i in 1..=10 {
            app.world_mut().resource_mut::<Time>().advance_by(Duration::from_secs(1));
            app.update();
            let (_, stats, _) = query.iter(app.world_mut()).next().unwrap();
            let health = stats.get(MyStats::Health).current_value;
            assert_eq!(health, 100. + (10. * i as f32));
            let events = app.world_mut().resource_mut::<Events<OnRepeatingEffectTriggered>>();
            let mut cursor = events.get_cursor();
            let events = cursor.read(&events);
            assert!(events.len() >= 1);
        }

        let events = app.world_mut().resource_mut::<Events<OnRepeatingEffectTriggered>>();
        let mut cursor = events.get_cursor();
        let event = cursor.read(&events).next().unwrap();
        assert_eq!(event.target_entity, entity);
        assert_eq!(event.effect_type, TypeId::of::<HealthRegen>());
            
        app.world_mut().resource_mut::<Time>().advance_by(Duration::from_secs(5));
        app.update();
        let (_, stats, _) = query.iter(app.world_mut()).next().unwrap();
        let health = stats.get(MyStats::Health).current_value;
        assert_eq!(health, 200.);

        let (_, _, active) = query.iter(app.world_mut()).next().unwrap();
        assert_eq!(active.0.len(), 0);
    }

    #[test] 
    fn test_continuous_with_nonlocal_magnitude() {
        let mut app = setup_app();
        let (entity1, _) = setup_entity(&mut app);
        let (entity2, mut query) = setup_entity(&mut app);

        struct Damage;
        let scaling = StatScalingParams {
            multiplier: -2.0,
            ..default()
        };
        app.world_mut().trigger(AddEffect(EffectMetadata::new(
            entity1, 
            StatEffect::new::<Damage>(
                MyStats::Health,
                EffectMagnitude::NonlocalStat(MyStats::Strength, scaling, entity2),
                EffectCalculation::Additive,
                EffectDuration::Continuous(Some(10.0.into())),
            )
        )));
        let (_, stats, _) = query.iter(app.world_mut()).next().unwrap();
        let health = stats.get(MyStats::Health).current_value;
        assert_eq!(health, 100.);

        app.world_mut().resource_mut::<Time>().advance_by(Duration::from_secs(5));
        app.update();
        for (entity, stats, _) in query.iter(app.world_mut()) {
            if entity != entity1 { continue; }
            let health = stats.get(MyStats::Health).current_value;
            assert_eq!(health, 0.);
        }

        app.world_mut().resource_mut::<Time>().advance_by(Duration::from_secs(5));
        app.update();
        for (entity, stats, _) in query.iter(app.world_mut()) {
            if entity != entity1 { continue; }
            let health = stats.get(MyStats::Health).current_value;
            assert_eq!(health, -100.);
        }
    }

    #[test] 
    fn test_persistent_removal() {
        let mut app = setup_app();
        let (entity, mut query) = setup_entity(&mut app);

        struct Damage;
        struct HealthBuff1;
        struct HealthBuff2;

        let buff1 = StatEffect::new::<HealthBuff1>(
            MyStats::Health,
            EffectMagnitude::Fixed(2.),
            EffectCalculation::Multiplicative,
            EffectDuration::Persistent(None),
        );
        let buff2 = StatEffect::new::<HealthBuff2>(
            MyStats::Health,
            EffectMagnitude::Fixed(2.),
            EffectCalculation::Multiplicative,
            EffectDuration::Persistent(None),
        );

        app.world_mut().trigger(AddEffect(EffectMetadata::new(entity, buff1.clone())));
        let (_, stats, _) = query.iter(app.world_mut()).next().unwrap();
        let health = stats.get(MyStats::Health).current_value;
        assert_eq!(health, 200.);
        app.world_mut().trigger(AddEffect(EffectMetadata::new(entity, buff2.clone())));
        let (_, stats, _) = query.iter(app.world_mut()).next().unwrap();
        let health = stats.get(MyStats::Health).current_value;
        assert_eq!(health, 400.);

        app.world_mut().trigger(AddEffect(EffectMetadata::new(
            entity, 
            StatEffect::new::<Damage>(
                MyStats::Health,
                EffectMagnitude::Fixed(-100.),
                EffectCalculation::Additive,
                EffectDuration::Immediate,
            )
        )));
        let (_, stats, _) = query.iter(app.world_mut()).next().unwrap();
        let health = stats.get(MyStats::Health).current_value;
        assert_eq!(health, 300.);
        
        app.world_mut().trigger(RemoveEffect(EffectTypeMetadata::new(entity, buff1.effect_type)));
        let (_, stats, _) = query.iter(app.world_mut()).next().unwrap();
        let health = stats.get(MyStats::Health).current_value;
        assert_eq!(health, 150.);

        app.world_mut().trigger(RemoveEffect(EffectTypeMetadata::new(entity, buff2.effect_type)));
        let (_, stats, _) = query.iter(app.world_mut()).next().unwrap();
        let health = stats.get(MyStats::Health).current_value;
        assert_eq!(health, 75.);
    }

    #[test] 
    fn test_no_stacking() {
        let mut app = setup_app();

        struct HealthDrain;
        let mut stacking_rules = HashMap::<TypeId, StackingPolicy>::new();
        stacking_rules.insert(TypeId::of::<HealthDrain>(), StackingPolicy::NoStacking);
        app.insert_resource(StackingBehaviors(stacking_rules));

        let (entity, mut query) = setup_entity(&mut app);

        let effect = StatEffect::new::<HealthDrain>(
            MyStats::Health,
            EffectMagnitude::Fixed(-1.0),
            EffectCalculation::Additive,
            EffectDuration::Continuous(Some(3.0.into())),
        );

        app.world_mut().trigger(AddEffect(EffectMetadata::new(entity, effect.clone())));
        app.world_mut().resource_mut::<Time>().advance_by(Duration::from_secs(1));
        app.update();
        
        let (_, stats, effects) = query.iter(app.world_mut()).next().unwrap();
        let health = stats.get(MyStats::Health).current_value;
        assert_eq!(health, 99.);
        assert_eq!(effects.0.iter().len(), 1);

        app.world_mut().trigger(AddEffect(EffectMetadata::new(entity, effect)));
        app.world_mut().resource_mut::<Time>().advance_by(Duration::from_secs(1));
        app.update();

        let (_, stats, effects) = query.iter(app.world_mut()).next().unwrap();
        let health = stats.get(MyStats::Health).current_value;
        assert_eq!(health, 98.);
        assert_eq!(effects.0.iter().len(), 1);
    }

    #[test] 
    fn test_no_stacking_reset_timer() {
        let mut app = setup_app();

        struct HealthDrain;
        let mut stacking_rules = HashMap::<TypeId, StackingPolicy>::new();
        stacking_rules.insert(TypeId::of::<HealthDrain>(), StackingPolicy::NoStackingResetDuration);
        app.insert_resource(StackingBehaviors(stacking_rules));

        let (entity, mut query) = setup_entity(&mut app);

        let effect = StatEffect::new::<HealthDrain>(
            MyStats::Health,
            EffectMagnitude::Fixed(-1.0),
            EffectCalculation::Additive,
            EffectDuration::Continuous(Some(3.0.into())),
        );

        for i in 0..5 {
            app.world_mut().trigger(AddEffect(EffectMetadata::new(entity, effect.clone())));
            app.world_mut().resource_mut::<Time>().advance_by(Duration::from_secs(1));
            app.update();

            let (_, stats, effects) = query.iter(app.world_mut()).next().unwrap();
            let health = stats.get(MyStats::Health).current_value;
            let target = 99.0 - i as f32;
            assert_eq!(health, target);
            assert_eq!(effects.0.iter().len(), 1);
        }
    }

    #[test] 
    fn test_multiple_effects_stacking() {
        let mut app = setup_app();

        struct HealthDrain;
        let mut stacking_rules = HashMap::<TypeId, StackingPolicy>::new();
        stacking_rules.insert(TypeId::of::<HealthDrain>(), StackingPolicy::MultipleEffects(3));
        app.insert_resource(StackingBehaviors(stacking_rules));

        let (entity, mut query) = setup_entity(&mut app);

        let effect = StatEffect::new::<HealthDrain>(
            MyStats::Health,
            EffectMagnitude::Fixed(-1.0),
            EffectCalculation::Additive,
            EffectDuration::Continuous(Some(5.0.into())),
        );

        let mut target = 100.;
        for i in 0..4 {
            app.world_mut().trigger(AddEffect(EffectMetadata::new(entity, effect.clone())));
            app.world_mut().resource_mut::<Time>().advance_by(Duration::from_secs(1));
            app.update();
            let n_effects: usize = usize::min(i+1, 3);
            target -= n_effects as f32;

            let (_, stats, effects) = query.iter(app.world_mut()).next().unwrap();
            let health = stats.get(MyStats::Health).current_value;
            assert_eq!(effects.0.iter().len(), n_effects);
            assert_eq!(health, target);
        }
        
        // effects should start timing out now
        let mut n_effects: i32 = 3;
        for _ in 0..6 {
            app.world_mut().resource_mut::<Time>().advance_by(Duration::from_secs(1));
            app.update();
            target -= n_effects as f32;
            n_effects = i32::max(0, n_effects - 1);

            let (_, stats, effects) = query.iter(app.world_mut()).next().unwrap();
            let health = stats.get(MyStats::Health).current_value;
            assert_eq!(effects.0.iter().len(), n_effects as usize);
            assert_eq!(health, target);
        }
    }

    #[test] 
    fn test_multiple_effects_reset_timers_stacking() {
        let mut app = setup_app();

        struct HealthDrain;
        let mut stacking_rules = HashMap::<TypeId, StackingPolicy>::new();
        stacking_rules.insert(TypeId::of::<HealthDrain>(), StackingPolicy::MultipleEffectsResetDurations(3));
        app.insert_resource(StackingBehaviors(stacking_rules));

        let (entity, mut query) = setup_entity(&mut app);

        let effect = StatEffect::new::<HealthDrain>(
            MyStats::Health,
            EffectMagnitude::Fixed(-1.0),
            EffectCalculation::Additive,
            EffectDuration::Continuous(Some(5.0.into())),
        );

        let mut target = 100.;
        for i in 0..8 {
            app.world_mut().trigger(AddEffect(EffectMetadata::new(entity, effect.clone())));
            app.world_mut().resource_mut::<Time>().advance_by(Duration::from_secs(1));
            app.update();
            let n_effects: usize = usize::min(i+1, 3);
            target -= n_effects as f32;

            let (_, stats, effects) = query.iter(app.world_mut()).next().unwrap();
            let health = stats.get(MyStats::Health).current_value;
            assert_eq!(effects.0.iter().len(), n_effects);
            assert_eq!(health, target);
        }
        
        app.world_mut().resource_mut::<Time>().advance_by(Duration::from_secs(5));
        app.update();
        target -= 15.;
        let (_, stats, effects) = query.iter(app.world_mut()).next().unwrap();
        let health = stats.get(MyStats::Health).current_value;
        assert_eq!(effects.0.iter().len(), 0);
        assert_eq!(health, target);
    }
}