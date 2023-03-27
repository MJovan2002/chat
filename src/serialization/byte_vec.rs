use aes_gcm_siv::aead::heapless;
use crate::serialization::{Buf, Deserializable, Deserializer, Serializable};

impl Serializable for &[u8] {
    type Serializer<'s> = Buf<'s> where Self: 's;

    fn serializer(&self) -> Self::Serializer<'_> {
        Buf::new(*self)
    }
}
impl Serializable for Vec<u8> {
    type Serializer<'s> = Buf<'s> where Self: 's;

    fn serializer(&self) -> Self::Serializer<'_> {
        Buf::new(self)
    }
}

impl Deserializable for Vec<u8> {
    type Deserializer = Vec<u8>;

    fn deserializer() -> Self::Deserializer {
        Self::Deserializer::new()
    }
}

impl Deserializer<Vec<u8>> for Vec<u8> {
    type UpdateError = !;
    type FinalizeError = !;

    fn update(&mut self, slice: &[u8]) -> Result<(), Self::UpdateError> {
        Ok(self.extend_from_slice(slice))
    }

    fn finalize(self) -> Result<Vec<u8>, Self::FinalizeError> {
        Ok(self)
    }
}

impl<const N: usize> Deserializer<[u8; N]> for heapless::Vec<u8, N> {
    type UpdateError = ();
    type FinalizeError = heapless::Vec<u8, N>;

    fn update(&mut self, slice: &[u8]) -> Result<(), Self::UpdateError> {
        self.extend_from_slice(slice)
    }

    fn finalize(self) -> Result<[u8; N], Self::FinalizeError> {
        self.into_array()
    }
}

impl<const N: usize> Deserializable for [u8; N] {
    type Deserializer = heapless::Vec<u8, N>;

    fn deserializer() -> Self::Deserializer {
        Self::Deserializer::new()
    }
}