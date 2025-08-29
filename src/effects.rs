use bevy::prelude::*;
use std::any::TypeId;
use smallvec::SmallVec;
use crate::{
    prelude::*,
    calculation::{apply_immediate, recalculate_stats},
    events::EffectTypeMetadata,
    timing::SmallTimer,
    StackingBehaviors
};

const ACTIVE_EFFECTS_SIZE: usize = 24;


#[derive(Clone)]
pub struct StatEffect<T: StatTrait> {
    pub stat_target: T,
    pub magnitude: EffectMagnitude<T>,
    pub calculation: EffectCalculation,
    pub duration: EffectDuration,
    pub effect_type: TypeId,
}

impl<T: StatTrait> StatEffect<T> {
    pub fn set_duration(&mut self, duration: impl Into<SmallTimer>) -> Result<(), &'static str> {
        match &mut self.duration {
            EffectDuration::Continuous(Some(timer)) => { timer.set_duration(duration); },
            EffectDuration::Persistent(Some(timer)) => { timer.set_duration(duration); },
            EffectDuration::Repeating(_, Some(timer)) => { timer.set_duration(duration); },
            _ => { return Err("Effect has no duration timer set") }
        }
        Ok(())
    }
}

impl<T: StatTrait> StatEffect<T> {
    pub fn new<U: Send + Sync + 'static>(
        stat_target: T,
        magnitude: EffectMagnitude<T>,
        calculation: EffectCalculation,
        duration: EffectDuration,
    ) -> Self {
        Self { stat_target, magnitude, calculation, duration, effect_type: TypeId::of::<U>() }
    }
}

impl<T: StatTrait> StatEffect<T> {
    fn get_duration_timer(&self) -> Option<&SmallTimer> {
        match &self.duration {
            EffectDuration::Continuous(Some(timer)) => Some(timer),
            EffectDuration::Persistent(Some(timer)) => Some(timer),
            EffectDuration::Repeating(_, Some(timer)) => Some(timer),
            _ => None
        }
    }
}

#[derive(Component, Clone)]
pub struct ActiveEffects<T: StatTrait>(pub(crate) SmallVec<[StatEffect<T>; ACTIVE_EFFECTS_SIZE]>);

impl<T: StatTrait> ActiveEffects<T> {
    pub fn new(effects: impl IntoIterator<Item = StatEffect<T>>) -> Self {
        let mut instance = Self(SmallVec::<[StatEffect<T>; ACTIVE_EFFECTS_SIZE]>::new());
        instance.0.extend(effects);
        instance
    }

    pub fn match_effect_type(&self, other: TypeId) -> impl Iterator<Item = &StatEffect<T>> {
        self.0.iter().filter(move |&e| e.effect_type == other)
    }
}


pub(crate) fn add_effect<T: StatTrait>(
    trigger: Trigger<AddEffect<T>>,
    mut stats_query: Query<&mut GameplayStats<T>>,
    mut active_effects: Query<(Entity, &mut ActiveEffects<T>)>,
    mut added_writer: EventWriter<OnEffectAdded<T>>,
    mut breached_writer: EventWriter<OnBoundsBreached<T>>,
    stacking_bahaviors: Res<StackingBehaviors>,
) {
    let event = trigger.event();
    let EffectMetadata::<T> { effect, target_entity} = &event.0;

    if let Ok((entity, mut effects)) = active_effects.get_mut(*target_entity) {
        match &effect.duration {
            EffectDuration::Immediate => {
                apply_immediate::<T>(entity, &effect, &effects, &mut stats_query, &mut breached_writer, None);
            },
            EffectDuration::Persistent(_) => {
                effects.0.push(effect.clone());
                recalculate_stats(entity, effect.stat_target, &effects, &mut breached_writer, &mut stats_query);
            },
            _ => {
                let stacking = stacking_bahaviors.0
                    .get(&effect.effect_type)
                    .cloned()
                    .unwrap_or_default();

                match stacking {
                    StackingPolicy::NoStacking => {
                        if effects.match_effect_type(effect.effect_type).count() == 0 {
                            effects.0.push(effect.clone());
                        }
                        return;
                    },
                    StackingPolicy::NoStackingResetDuration => {
                        if effects.match_effect_type(effect.effect_type).count() == 0 {
                            effects.0.push(effect.clone());
                        } else if let Some(timer) = effect.get_duration_timer() {
                            for other in effects.0.iter_mut() {
                                other.set_duration(timer.clone()).ok();
                            }
                        }
                        return;
                    }
                    StackingPolicy::MultipleEffects(max) => {
                        if effects.match_effect_type(effect.effect_type).count() < max as usize {
                            effects.0.push(effect.clone());
                        }
                    },
                    StackingPolicy::MultipleEffectsResetDurations(max) => {
                        if effects.match_effect_type(effect.effect_type).count() < max as usize {
                            effects.0.push(effect.clone());
                        }
                        if let Some(timer) = effect.get_duration_timer() {
                            for other in effects.0.iter_mut() {
                                other.set_duration(timer.clone()).ok();
                            }
                        }
                    },
                }
            }
        }
        added_writer.write(OnEffectAdded(
            EffectTypeMetadata::new(
                event.0.target_entity,
                event.0.effect.effect_type
            )
        ));
    }
}

pub(crate) fn process_active_effects<T: StatTrait>(
    time: Res<Time>,
    mut stats_query: Query<&mut GameplayStats<T>>,
    mut entity_effects_query: Query<(Entity, &mut ActiveEffects<T>)>,
    mut periodic_event_writer: EventWriter<OnRepeatingEffectTriggered<T>>,
    mut breached_writer: EventWriter<OnBoundsBreached<T>>,
    mut commands: Commands,
) {
    entity_effects_query.iter_mut().for_each(|(entity, mut effects)| {
        for effect in effects.0.iter_mut() {
            match &mut effect.duration {
                EffectDuration::Continuous(Some(timer)) => { timer.tick(time.delta_secs()); },
                EffectDuration::Persistent(Some(timer)) => { timer.tick(time.delta_secs()); },
                EffectDuration::Repeating(period, timer) => {
                    period.tick(time.delta_secs());
                    if let Some(timer) = timer {
                        timer.tick(time.delta_secs());
                    }
                },
                _ => {}
            }
        }
        
        let mut to_remove: Option<SmallVec::<[usize; 8]>> = None;

        for (idx, effect) in effects.0.iter().enumerate() {
            match &effect.duration {
                EffectDuration::Continuous(timer) => {
                    apply_immediate::<T>(entity, &effect, &effects, &mut stats_query, &mut breached_writer, Some(time.delta_secs()));
                    if let Some(timer) = timer {
                        if timer.finished() {
                            to_remove.get_or_insert_default().push(idx);
                            commands.trigger(OnEffectRemoved(EffectTypeMetadata::<T>::new(entity, effect.effect_type)));
                        }
                    }
                },
                EffectDuration::Persistent(timer) => {
                    if matches!(effect.calculation, EffectCalculation::LowerBound) ||
                            matches!(effect.calculation, EffectCalculation::UpperBound) {
                        apply_immediate::<T>(entity, &effect, &effects, &mut stats_query, &mut breached_writer, None);
                    }
                    if let Some(timer) = timer {
                        if timer.finished() {
                            to_remove.get_or_insert_default().push(idx);
                            commands.trigger(OnEffectRemoved(EffectTypeMetadata::<T>::new(entity, effect.effect_type)));
                        }
                    }
                },
                EffectDuration::Repeating(period, duration) => {
                    if period.just_triggered() {
                        apply_immediate::<T>(entity, &effect, &effects, &mut stats_query, &mut breached_writer, None);
                        periodic_event_writer.write(OnRepeatingEffectTriggered(
                            EffectTypeMetadata::new(entity, effect.effect_type)
                        ));
                    }
                    if let Some(timer) = duration {
                        if timer.finished() {
                            to_remove.get_or_insert_default().push(idx);
                            commands.trigger(OnEffectRemoved(EffectTypeMetadata::<T>::new(entity, effect.effect_type)));
                        }
                    }
                },
                _ => (),
            };
        };
        if let Some(to_remove) = to_remove {
            for &i in to_remove.iter().rev() {
                let effect = effects.0.remove(i);
                if matches!(effect.duration, EffectDuration::Persistent(_)) {
                    recalculate_stats(entity, effect.stat_target, &effects, &mut breached_writer, &mut stats_query);
                }
            }
        }
    });
}


pub(crate) fn remove_effect<T: StatTrait>(
    trigger: Trigger<RemoveEffect<T>>,
    mut breached_writer: EventWriter<OnBoundsBreached<T>>,
    mut effects_entities_query: Query<&mut ActiveEffects<T>>,
    mut stats_query: Query<&mut GameplayStats<T>>,
) {
    let event = trigger.event();
    let EffectMetadata{ effect, target_entity: entity } = &event.0;
    let mut effects = effects_entities_query.get_mut(*entity)
        .expect("Failed to get entity");
    let mut to_remove = SmallVec::<[usize; 8]>::new();
    for (index, current_effect) in effects.0.iter().enumerate() {
        if effect.effect_type == current_effect.effect_type {
            to_remove.push(index);
        }
    }
    for &i in to_remove.iter().rev() {
        let effect = effects.0.remove(i);
        recalculate_stats(*entity, effect.stat_target, &effects, &mut breached_writer, &mut stats_query);
    }
}
