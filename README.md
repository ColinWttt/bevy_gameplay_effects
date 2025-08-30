# Bevy Stat Effects
Gameplay Stats and Effects for the Bevy game engine.  Inspired by GameplayAttributes from UE5's GameplayAbilitySystem.

## Features
- GameplayStats component to track entity stats
- ActiveEffects component to modify stats at runtime
- Dynamic stat magnitude based on other stats, possibly on other entities
- Effects can add, multiply or clamp stat values
- Persistent, immediate, continuous, or repeating effects with optional durations
- Stat effect events for syncing gameplay cues, audio, animation, particles, etc.
- Effect stacking rules


# Stat Representation
Stats are represented as f32. GameplayStat is a struct that wraps a few f32, including the current and base values.  Base values are good for things like levelling up, but they are also necessary for deterministic behavior of revertible persistent effects.  If we didn't store some other state, then there could be hysteresis or path dependent effects by repeatedly applying and removing a mixture of additive and multiplicative buffs, which can lead to player exploits to order to achieve unreasonable stats, but you don't have to worry about any of this. 

Stat types are represented as user-defined enums.  Use the stats! macro to define them (see examples).  This will impl some traits includeing Into\<u8\>.

GameplayStats\<T\> is a component that holds a [GameplayStat; 16], where T is your stat enum type.  It is currently fixed size and not extendable, so it will always have the same size no matter how many stats you actually use.  The goal was to keep things as cache friendly as possible for iteration.  When you call GameplayStats::\<YourStatEnum\>::new you feed in an initializer function to set the initial stat values.  Due to a limitation in rust, the compiler cannot iterate over enum variants, so you must supply an iterator over your enum.  Internally enum variants are cast to u8 to access the underlying array.  Because of this, You should always set the variants in the order they are defined in your enum, with none missing, except possibly truncation at the end.

The StatEffectsPlugin is generic over your stats enum, so you could have more than 1 if desired for some reason.  It also takes in a StackingBehavior resource.  See below.

# StatEffects
StatEffect\<YourStatEnum\> is a struct that carries data related to how the effect should change your stat.  It holds a duration, a magnitude, a calculation, a stat target, and a TypeId (used for identifying or removing a stat by type).  The stat_target is just the stat enum variant that the effect is targeting.  If you need to target multiple stats, use multiple effects.

## EffectDurations
- Immediate effects are applied and then discarded, useful for things like taking damage or restoring health with a potion.
- Persistent effects are the only ones that reverse their effect when removed, useful for things like equipment based stat buffs.  They can also have an optional timer to automatically remove later.
- Continuous effects apply every Update.  Also they are unique in that the magnitude of contiuous effects is expressed as amount/second rather than an absolute amount. You can supply an optional timer.
- Repeating effects trigger periodically.  They require a timer for the period and can accept a second timer for the duration.

Bevy timers are rather large, so I wrote a custom SmallTimer type.  But it impl From f32 so you can just do 10.0.into()
  
## EffectCalculation
Stat effects have several different calculation modes which alter the stats in different ways
- Additive (10 means add 10 to the current stat value)
- Multiplicative (1.1 means add 10% to the current stat value)
- LowerBound (prevent the stat from going below a minimum value)
- UpperBound (prevent the stat from going above a maximum value)
- SetValue (sets the value of the stat directly, still constrained by any bounds in place)

## EffectMagnitude
Stat effects can have static or dynamic magnitudes
- Fixed(f32)
- LocalStat(T, f32) depends on a stat on the same entity, e.g. drive a health regeneration effect based on a HealthRegen stat type
- NonLocalStat(T, f32, Entity) depends on a stat on another entity, e.g. do damage according to the source's strength stat.
  
For an effect that depends on other stats, you could also pre-calculate a Fixed amount.  The difference with the last two magnitude variants is that they are dynamic.  For example, an entity is doing continuous damage to another entity over 10 seconds.  Halfway through it levels up and it's damage stat increaes.  The same effect is now doing more damage for the remainder of the effect without any intervention on your part.  This does come at a cost though.  NonLocalStats are the reason I cannot do multithreading in the effect system because of the borrow rules with queries.  If the entity inside a NonLocalStat ceases to exist, the effect is removed.
### StatScalingParams
When doing stat based effect scaling, you may not want to scale your effect magnitude with the underlying stat directly. StatScalingParams is a simple struct with an apply() method, which can transform the stat into a magnitude.  It is defined like this
```
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
```
I thought this approach would be cheaper than using some Box\<dyn T\> though it is more limited, but probably flexible enough.

# Stacking
The StatEffectsPlugin requires a StackingBehavior resource to initialize, although you can use ::default() if you don't want any stacking.  This is just a hashmap from effect TypeId to a StackingPolicy.

There are a few stacking policies supported.  Currently stacking is only linear, i.e. each effect will have the same magnitude.  You could get around this by defining different stats with the same underlying TypeId, but right now I don't have support for dynamic scaling of magnitudes based on the number of stacked effects.

- NoStacking
- NoStackingResetTimer
- MultipleEffects(n) <- here n is the max number of effects you can stack
- MultipleEffectsResetTimer(n)

Basically you can either stack effects (up to n) or not stack at all.  Optionally you can reset all effect timers, e.g. if a character has an OnFire effect and walks into fire again, you may want to reset the timer for this effect.

# ActiveEffects
ActiveEffects\<T\> is a component that holds all the effects on an entity.  The entity must also have a GameplayStats\<T\> component.  Internally this is represented as a SmallVec of size 24.  Here you can exceed 24 effects but performance will degrade.

# Events
### Triggers
AddEffect and RemoveEffect are used to manually add and remove effects.  When you use RemoveEffect, all effects matching the supplied effect type will be removed.

### Feedback Events
Entities can react to stat effect events by listening to the following

- OnBoundsBreached\<T\>. This fires whenever a stat reached a limit defined by an upper/lower bound effect. Useful for death or overcharge effects.
- OnRepeatingEffectTriggered
- OnEffectAdded
- OnEffectRemoved
