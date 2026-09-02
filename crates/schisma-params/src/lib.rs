//! Stable parameter metadata shared by Schisma's engine, graph, schema, and UI.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use thiserror::Error;

/// A permanent numeric parameter identifier. Released IDs are never reused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ParamId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ParamUnit {
    Normalized,
    Hertz,
    Decibels,
    Seconds,
    Semitones,
    Percent,
    Voices,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SmoothingClass {
    None,
    Fast,
    Musical,
    Slow,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParamDescriptor {
    pub id: ParamId,
    pub key: String,
    pub name: String,
    pub minimum: f64,
    pub maximum: f64,
    pub default: f64,
    pub unit: ParamUnit,
    pub smoothing: SmoothingClass,
}

impl ParamDescriptor {
    pub fn validate(&self) -> Result<(), ParamError> {
        if self.key.trim().is_empty() {
            return Err(ParamError::EmptyKey(self.id));
        }
        if !self.minimum.is_finite()
            || !self.maximum.is_finite()
            || !self.default.is_finite()
            || self.minimum >= self.maximum
        {
            return Err(ParamError::InvalidRange(self.id));
        }
        if !(self.minimum..=self.maximum).contains(&self.default) {
            return Err(ParamError::DefaultOutOfRange(self.id));
        }
        Ok(())
    }

    pub fn clamp(&self, value: f64) -> f64 {
        value.clamp(self.minimum, self.maximum)
    }

    pub fn normalized(&self, value: f64) -> f64 {
        (self.clamp(value) - self.minimum) / (self.maximum - self.minimum)
    }
}

#[derive(Debug, Clone, Default)]
pub struct ParamRegistry {
    by_id: BTreeMap<ParamId, ParamDescriptor>,
    by_key: BTreeMap<String, ParamId>,
}

impl ParamRegistry {
    pub fn register(&mut self, descriptor: ParamDescriptor) -> Result<(), ParamError> {
        descriptor.validate()?;
        if self.by_id.contains_key(&descriptor.id) {
            return Err(ParamError::DuplicateId(descriptor.id));
        }
        if self.by_key.contains_key(&descriptor.key) {
            return Err(ParamError::DuplicateKey(descriptor.key));
        }
        self.by_key.insert(descriptor.key.clone(), descriptor.id);
        self.by_id.insert(descriptor.id, descriptor);
        Ok(())
    }

    pub fn get(&self, id: ParamId) -> Option<&ParamDescriptor> {
        self.by_id.get(&id)
    }

    pub fn by_key(&self, key: &str) -> Option<&ParamDescriptor> {
        self.by_key.get(key).and_then(|id| self.by_id.get(id))
    }

    pub fn iter(&self) -> impl Iterator<Item = &ParamDescriptor> {
        self.by_id.values()
    }
}

#[derive(Debug, Error, PartialEq)]
pub enum ParamError {
    #[error("parameter {0:?} has an empty key")]
    EmptyKey(ParamId),
    #[error("parameter {0:?} has an invalid range")]
    InvalidRange(ParamId),
    #[error("parameter {0:?} has a default outside its range")]
    DefaultOutOfRange(ParamId),
    #[error("duplicate parameter ID {0:?}")]
    DuplicateId(ParamId),
    #[error("duplicate parameter key '{0}'")]
    DuplicateKey(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn morph() -> ParamDescriptor {
        ParamDescriptor {
            id: ParamId(1),
            key: "voice.morph".into(),
            name: "Morph".into(),
            minimum: 0.0,
            maximum: 1.0,
            default: 0.5,
            unit: ParamUnit::Normalized,
            smoothing: SmoothingClass::Musical,
        }
    }

    #[test]
    fn registry_resolves_stable_ids_and_keys() {
        let mut registry = ParamRegistry::default();
        registry.register(morph()).unwrap();
        assert_eq!(registry.get(ParamId(1)).unwrap().name, "Morph");
        assert_eq!(registry.by_key("voice.morph").unwrap().id, ParamId(1));
    }

    #[test]
    fn duplicate_ids_are_rejected() {
        let mut registry = ParamRegistry::default();
        registry.register(morph()).unwrap();
        let mut duplicate = morph();
        duplicate.key = "other".into();
        assert_eq!(
            registry.register(duplicate),
            Err(ParamError::DuplicateId(ParamId(1)))
        );
    }
}
