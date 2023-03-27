use crate::serialization::{Deserializable, Deserializer, Serializable, Serializer};

impl Serializer for () {
    fn fill(&mut self, _: &mut [u8]) -> Option<usize> {
        Some(0)
    }
}

impl Serializable for () {
    type Serializer<'s> = ();

    fn serializer(&self) -> Self::Serializer<'_> {}
}

impl Deserializer<()> for bool {
    type UpdateError = ();
    type FinalizeError = ();

    fn update(&mut self, slice: &[u8]) -> Result<(), Self::UpdateError> {
        match (*self, slice) {
            (false, []) => Ok(*self = true),
            _ => Err(()),
        }
    }

    fn finalize(self) -> Result<(), Self::FinalizeError> {
        match self {
            true => Ok(()),
            false => Err(()),
        }
    }
}

impl Deserializable for () {
    type Deserializer = bool;

    fn deserializer() -> Self::Deserializer {
        false
    }
}
