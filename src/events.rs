use std::any::TypeId;
use bevy::prelude::*;
use crate::prelude::*;

#[derive(Clone)]
pub struct EffectMetadata<T: StatTrait> {
    pub target_entity: Entity,
    pub effect: StatEffect<T>,
}

impl<T: StatTrait> EffectMetadata<T> {
    pub fn new(entity: Entity, effect: StatEffect<T>) -> Self {
        Self { effect, target_entity: entity }
    }
}

pub struct EffectTypeMetadata {
    pub target_entity: Entity,
    pub effect_type: TypeId,
}

impl EffectTypeMetadata {
    pub fn new(entity: Entity, effect_type: TypeId) -> Self {
        Self { target_entity: entity, effect_type }
    }
}

pub struct BoundsBreachedMetadata<T> {
    pub target_entity: Entity,
    pub stat: T,
    pub bound: EffectCalculation,
}

impl<T: StatTrait> BoundsBreachedMetadata<T> {
    pub fn new(entity: Entity, stat: T, bound: EffectCalculation) -> Self {
        Self { target_entity: entity, stat, bound }
    }
}

#[derive(Event, Deref)]
pub struct AddEffect<T: StatTrait>(pub EffectMetadata<T>);

#[derive(Event, Deref)]
pub struct RemoveEffect(pub EffectTypeMetadata);

#[derive(Event, Deref)]
pub struct OnEffectAdded(pub EffectTypeMetadata);

#[derive(Event, Deref)]
pub struct OnEffectRemoved(pub EffectTypeMetadata);

#[derive(Event, Deref)]
pub struct OnRepeatingEffectTriggered(pub EffectTypeMetadata);

#[derive(Event, Deref)]
pub struct OnBoundsBreached<T: StatTrait>(pub BoundsBreachedMetadata<T>);