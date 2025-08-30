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
    stats_query: &mut Query<&mut GameplayStats<T>>,
    amount: f32,
    upper_bound: f32,
    lower_bound: f32,
) -> Option<OnBoundsBreached<T>> {

    let mut stats = stats_query.get_mut(entity).expect("Missing GameplayStats component");
    let stat = stats.get_mut(effect.stat_target);

    match &effect.calculation {
        EffectCalculation::Additive => { stat.current_value += amount },
        EffectCalculation::Multiplicative => { stat.current_value *= amount },
        _ => { }
    }
    if stat.current_value >= upper_bound {
        stat.current_value = upper_bound;
        Some(OnBoundsBreached(BoundsBreachedMetadata::new(entity, effect.stat_target, EffectCalculation::UpperBound)))
    } else if stat.current_value <= lower_bound {
        stat.current_value = lower_bound;
        Some(OnBoundsBreached(BoundsBreachedMetadata::new(entity, effect.stat_target, EffectCalculation::LowerBound)))
    } else { None }
}

/// After persistent effects are added/removed recalulate base and current stat values
pub(crate) fn recalculate_stats<T: StatTrait>(
    entity: Entity,
    effects: &Mut<ActiveEffects<T>>,
    stat_target: T, 
    stats_query: &mut Query<&mut GameplayStats<T>>,
    upper_bound: f32,
    lower_bound: f32
) -> Option<OnBoundsBreached<T>> {
    let mut additive: f32 = 0.;
    let mut multiplicative: f32 = 1.;

    for effect in effects.0.iter() {
        let source = get_effect_source(effect, entity, stats_query);
        let amount = get_effect_amount(entity, effect, source);
        
        if effect.stat_target == stat_target {
            match effect.calculation {
                EffectCalculation::Additive => { additive += amount },
                EffectCalculation::Multiplicative => { multiplicative *= amount },
                _ => { }
            }
        }
    }

    let mut stats = stats_query.get_mut(entity)
        .expect("No stats component found");
    let stat = stats.get_mut(stat_target);
    let prev_base = stat.modified_base;
    let mut new_base = (stat.base_value + additive) * multiplicative;
    new_base = f32::min(upper_bound, new_base);
    new_base = f32::max(lower_bound, new_base);
    stat.modified_base = new_base;
    stat.current_value *= new_base / prev_base;

    if stat.current_value >= upper_bound {
        stat.current_value = upper_bound;
        Some(OnBoundsBreached(
            BoundsBreachedMetadata {
                stat: stat_target,
                bound: EffectCalculation::UpperBound,
                target_entity: entity,
            }
        ))
    } else if stat.current_value <= lower_bound {
        stat.current_value = lower_bound;
        Some(OnBoundsBreached(
            BoundsBreachedMetadata {
                stat: stat_target,
                bound: EffectCalculation::LowerBound,
                target_entity: entity,
            }
        ))
    } else { None }
}

/// Get the magnitude of the effect on the stat
pub(crate) fn get_effect_amount<T:StatTrait>(
    entity: Entity,
    effect: &StatEffect<T>,
    source: Option<&GameplayStats<T>>,
)  -> f32 {
    match &effect.magnitude {
        EffectMagnitude::Fixed(x) => *x,
        EffectMagnitude::LocalStat(stat, f) => {
            let stats = source.unwrap();
            f.apply(stats.get(*stat).current_value)
        },
        EffectMagnitude::NonlocalStat(stat, f, _) => {
            let stats = source.unwrap();
            f.apply(stats.get(*stat).current_value)
        },
    }
}

pub(crate) fn get_effect_source<'a, T: StatTrait>(
    effect: &StatEffect<T>,
    entity: Entity,
    stats_query: &'a mut Query<&mut GameplayStats<T>>,
) -> Option<&'a GameplayStats<T>> {
    match &effect.magnitude {
        EffectMagnitude::NonlocalStat(_, _, source_entity) => {
            if let Ok(stats) = stats_query.get(*source_entity) {
                return Some(stats)
            } else { return None; }
        },
        EffectMagnitude::LocalStat(..) => return stats_query.get(entity).ok(),
        _ => return None,
    };
}