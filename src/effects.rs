use bevy::prelude::*;
use std::any::TypeId;
use smallvec::SmallVec;
use crate::{
    prelude::*,
    calculation::{apply_immediate, get_effect_amount, get_effect_source, recalculate_stats},
    events::EffectTypeMetadata,
    timing::SmallTimer, StackingBehaviors
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

#[derive(Component, Clone, Deref, DerefMut)]
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
    mut added_writer: EventWriter<OnEffectAdded>,
    mut breached_writer: EventWriter<OnBoundsBreached<T>>,
    stacking_bahaviors: Res<StackingBehaviors>,
) {
    let event = trigger.event();
    let EffectMetadata::<T> { effect, target_entity} = &event.0;

    if let Ok((entity, mut effects)) = active_effects.get_mut(*target_entity) {
        let source = get_effect_source(effect, entity, &mut stats_query);
        let amount = get_effect_amount(effect, source);
            
        match &effect.duration {

            EffectDuration::Immediate => {
                if let Some(e) = apply_immediate(entity, effect,
                                            &mut stats_query, amount, &effects) {
                    breached_writer.write(e);
                }
            },
            EffectDuration::Persistent(_) => {
                effects.0.push(effect.clone());
                if let Some(e) = recalculate_stats(entity, &effects, effect.stat_target, &mut stats_query) {
                    breached_writer.write(e);
                }
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

pub(crate) fn remove_effect<T: StatTrait>(
    trigger: Trigger<RemoveEffect>,
    mut breached_writer: EventWriter<OnBoundsBreached<T>>,
    mut removed_writer: EventWriter<OnEffectRemoved>,
    mut effects_entities_query: Query<&mut ActiveEffects<T>>,
    mut stats_query: Query<&mut GameplayStats<T>>,
) {
    let event = trigger.event();
    let EffectTypeMetadata{ effect_type, target_entity: entity, .. } = &event.0;
    let mut effects = effects_entities_query.get_mut(*entity)
        .expect("Failed to get entity");
    let mut to_remove = SmallVec::<[usize; 8]>::new();
    for (index, current_effect) in effects.0.iter().enumerate() {
        if *effect_type == current_effect.effect_type {
            to_remove.push(index);
        }
    }

    for &i in to_remove.iter().rev() {
        let effect = effects.0.remove(i);
        if let Some(e) = recalculate_stats(*entity, &effects, effect.stat_target, &mut stats_query) {
            breached_writer.write(e);
        }
        removed_writer.write(OnEffectRemoved(EffectTypeMetadata::new(event.target_entity, effect.effect_type)));
    }
}

pub(crate) fn process_active_effects<T: StatTrait>(
    time: Res<Time>,
    mut stats_query: Query<&mut GameplayStats<T>>,
    mut entity_effects_query: Query<(Entity, &mut ActiveEffects<T>)>,
    mut periodic_event_writer: EventWriter<OnRepeatingEffectTriggered>,
    mut breached_writer: EventWriter<OnBoundsBreached<T>>,
    mut removed_writer: EventWriter<OnEffectRemoved>,
) {
    entity_effects_query.iter_mut().for_each(|(entity, mut effects)| {

        // Tick all the timers
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
        
        let mut to_remove = SmallVec::<[usize; 8]>::new();

        // Now apply effects for this frame
        for (idx, effect) in effects.0.iter().enumerate() {
            // Get effect magnitude
            let source = get_effect_source(effect, entity, &mut stats_query);
            if matches!(effect.magnitude, EffectMagnitude::NonlocalStat(..)) && source.is_none() { // Source entity gone
                to_remove.push(idx); 
            }
            let mut amount = get_effect_amount(effect, source);
            if matches!(effect.duration, EffectDuration::Continuous(_)) {
                amount *= time.delta_secs();
                // TODO check effect saturation so framerate spikes don't cause a huge effect
            }

            // Check for expiration timers
            if let Some(timer) = effect.get_duration_timer() {
                if timer.finished() {
                    to_remove.push(idx);
                }
            }

            // Persistent and immediate effects are already applied
            let apply = match effect.duration {
                EffectDuration::Repeating(period, _) => {
                    if period.just_triggered() {
                        periodic_event_writer.write(OnRepeatingEffectTriggered(EffectTypeMetadata::new(
                            entity, effect.effect_type
                        )));
                        true
                    } else { false }
                },
                EffectDuration::Continuous(_) => { true },
                _ => { false }
            };
            if apply {
                if let Some(event) = apply_immediate(entity, effect, &mut stats_query, amount, &effects) {
                    breached_writer.write(event);
                }
            }
        }

        for &i in to_remove.iter().rev() {
            let effect = effects.0.remove(i);
            removed_writer.write(OnEffectRemoved(EffectTypeMetadata::new(entity, effect.effect_type)));
        }
    });
}