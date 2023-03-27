use crate::serialization::{Deserializable, Serializable};

pub trait Message: Serializable + Deserializable + Clone {}
