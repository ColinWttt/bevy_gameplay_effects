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
