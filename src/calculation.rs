use crate::prelude::*;
use bevy::prelude::*;


#[derive(Default, Copy, Clone)]
pub enum StackingPolicy {
    #[default]
    NoStacking,
    NoStackingResetDuration,
    MultipleEffects(u8),
    MultipleEffectsResetDurations(u8),
}

#[derive(Clone, PartialEq)]
pub enum EffectMagnitude<T: StatTrait> {
    Fixed(f32),
    LocalStat(T, StatScalingParams),
    NonlocalStat(T, StatScalingParams, Entity),
}

#[derive(Clone, PartialEq)]
pub enum EffectCalculation {
    Additive,
    Multiplicative,
    LowerBound,
    UpperBound,
}

#[derive(Clone, PartialEq)]
pub struct StatScalingParams {
    pub shift: f32,
    pub stat_offset: f32,
    pub multiplier: f32,
    pub exponent: f32,
    pub min: Option<f32>,
    pub max: Option<f32>,
}

impl Default for StatScalingParams {
    fn default() -> Self {
        Self {
            shift: 0.0,
            stat_offset: 0.0,
            multiplier: 1.0,
            exponent: 1.0,
            min: None,
            max: None
        }
    }
}

impl StatScalingParams {
    pub(crate) fn apply(&self, stat: f32) -> f32 {
        let mut out = self.shift + self.multiplier * (stat - self.stat_offset).powf(self.exponent);
        if let Some(min) = self.min {
            out = f32::max(min, out);
        }
        if let Some(max) = self.max {
            out = f32::min(max, out);
        }
        out
    }
}

/// Apply changes to a stat's current value
pub(crate) fn apply_immediate<T: StatTrait> (
    entity: Entity,
    effect: &StatEffect<T>, 
    effects: &ActiveEffects<T>,
    stats_query: &mut Query<&mut GameplayStats<T>>,
    breached_writer: &mut EventWriter<OnBoundsBreached<T>>,
    amount_mult: Option<f32>,
) {
    let (upper_bound, lower_bound) = get_bounds(effect.stat_target, entity, effects, stats_query);

    let mut amount = get_effect_amount(entity, effect, stats_query);
    if let Some(mult) = amount_mult { amount *= mult; }

    let mut stats = stats_query.get_mut(entity).expect("Failed to get stats for entitiy");
    let stat = stats.get_mut(effect.stat_target);

    match &effect.calculation {
        EffectCalculation::Additive => { stat.current_value += amount },
        EffectCalculation::Multiplicative => { stat.current_value *= amount },
        EffectCalculation::LowerBound => { stat.current_value = f32::max(stat.current_value, amount) },
        EffectCalculation::UpperBound => { stat.current_value = f32::min(stat.current_value, amount) },
    }
    if stat.current_value >= upper_bound {
        breached_writer.write(OnBoundsBreached(
            BoundsBreachedMetadata::new(entity, effect.stat_target, EffectCalculation::UpperBound))
        );
        stat.current_value = upper_bound;
    }
    if stat.current_value <= lower_bound {
        breached_writer.write(OnBoundsBreached(
            BoundsBreachedMetadata::new(entity, effect.stat_target, EffectCalculation::LowerBound))
        );
        stat.current_value = lower_bound;
    }
}

/// After persistent effects are added/removed recalulate base and current stat values
pub(crate) fn recalculate_stats<T: StatTrait>(
    entity: Entity,
    stat_target: T, 
    effects: &Mut<ActiveEffects<T>>,
    breached_writer: &mut EventWriter<OnBoundsBreached<T>>,
    stats_query: &mut Query<&mut GameplayStats<T>>,
) {
    let mut additive: f32 = 0.;
    let mut multiplicative: f32 = 1.;
    let mut lower_bound: f32 = f32::MIN;
    let mut upper_bound: f32 = f32::MAX;

    for effect in effects.0.iter() {
        let amount = get_effect_amount(entity, effect, stats_query);
        
        if effect.stat_target == stat_target {
            match effect.calculation {
                EffectCalculation::Additive => { additive += amount },
                EffectCalculation::Multiplicative => { multiplicative *= amount },
                EffectCalculation::LowerBound => { lower_bound = f32::max(lower_bound, amount) },
                EffectCalculation::UpperBound => { upper_bound = f32::min(upper_bound, amount) },
            }
        }
    }

    let mut stats = stats_query.get_mut(entity)
        .expect("Failed to get entity stats");
    let stat = stats.get_mut(stat_target);
    let prev_base = stat.modified_base;
    let mut new_base = (stat.base_value + additive) * multiplicative;
    new_base = f32::min(upper_bound, new_base);
    new_base = f32::max(lower_bound, new_base);
    stat.modified_base = new_base;
    stat.current_value *= new_base / prev_base;

    if stat.current_value >= upper_bound {
        breached_writer.write(OnBoundsBreached(
            BoundsBreachedMetadata {
                stat: stat_target,
                bound: EffectCalculation::UpperBound,
                target_entity: entity,
            }
        ));
        stat.current_value = upper_bound;
    }
    if stat.current_value <= lower_bound {
        breached_writer.write(OnBoundsBreached(
            BoundsBreachedMetadata {
                stat: stat_target,
                bound: EffectCalculation::LowerBound,
                target_entity: entity,
            }
        ));
        stat.current_value = lower_bound;
    }
}

/// Get the magnitude of the effect on the stat
pub(crate) fn get_effect_amount<T:StatTrait>(
    entity: Entity,
    effect: &StatEffect<T>,
    stats_query: &Query<&mut GameplayStats<T>>,
)  -> f32 {
    match &effect.magnitude {
        EffectMagnitude::Fixed(x) => *x,
        EffectMagnitude::LocalStat(stat, f) => {
            let stats = stats_query.get(entity)
                .expect("Failed to get stats");
            f.apply(stats.get(*stat).current_value)
        },
        EffectMagnitude::NonlocalStat(stat, f, source) => {
            let source_stats = stats_query.get(*source)
                .expect("Failed to get source entity for stat based effect");
            f.apply(source_stats.get(*stat).current_value)
        },
    }
}


/// Get the upper/lower bounds for a given stat with given active effects
pub(crate) fn get_bounds<T: StatTrait>(
    stat_variant: T,
    target_entity: Entity,
    effects: &ActiveEffects<T>,
    stats_query: &mut Query<&mut GameplayStats<T>>,
) -> (f32, f32) {
    let mut upper_bound = f32::MAX;
    let mut lower_bound = f32::MIN;
    for effect in effects.0.iter() {
        if effect.stat_target != stat_variant { continue; }
        match effect.calculation {
            EffectCalculation::LowerBound => {
                let amount = get_effect_amount(
                    target_entity,
                    &effect,
                    &stats_query
                );
                if amount > lower_bound {
                    lower_bound = amount;
                }
            },
            EffectCalculation::UpperBound => {
                let amount = get_effect_amount(
                    target_entity,
                    &effect,
                    &stats_query
                );
                if amount < upper_bound {
                    upper_bound = amount;
                }
            },
            _ => { }
        }
    }
    (upper_bound, lower_bound)
}