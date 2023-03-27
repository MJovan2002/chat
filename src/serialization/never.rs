use crate::serialization::{Deserializable, Deserializer, Serializable, Serializer};

impl Serializer for ! {
    fn fill(&mut self, _: &mut [u8]) -> Option<usize> {
        *self
    }
}

impl Serializable for ! {
    type Serializer<'s> = !;

    fn serializer(&self) -> Self::Serializer<'_> {
        *self
    }
}

impl Deserializer<!> for ! {
    type UpdateError = !;
    type FinalizeError = !;

    fn update(&mut self, _: &[u8]) -> Result<(), Self::UpdateError> {
        *self
    }

    fn finalize(self) -> Result<!, Self::FinalizeError> {
        self
    }
}

impl Deserializable for ! {
    type Deserializer = !;

    fn deserializer() -> Self::Deserializer {
        panic!()
    }
}