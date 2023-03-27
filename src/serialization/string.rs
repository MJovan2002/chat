use std::string::FromUtf8Error;
use crate::serialization::{Buf, Deserializable, Deserializer, Serializable};

impl Serializable for String {
    type Serializer<'s> = Buf<'s>;

    fn serializer(&self) -> Self::Serializer<'_> {
        Buf {
            buf: self.as_bytes(),
            pos: 0,
        }
    }
}

impl Serializable for str {
    type Serializer<'s> = Buf<'s>;

    fn serializer(&self) -> Self::Serializer<'_> {
        Buf {
            buf: self.as_bytes(),
            pos: 0,
        }
    }
}

impl Serializable for &str {
    type Serializer<'s> = Buf<'s> where Self: 's;

    fn serializer(&self) -> Self::Serializer<'_> {
        Buf {
            buf: self.as_bytes(),
            pos: 0,
        }
    }
}

impl Deserializer<String> for Vec<u8> {
    type UpdateError = !;
    type FinalizeError = FromUtf8Error;

    fn update(&mut self, slice: &[u8]) -> Result<(), Self::UpdateError> {
        Ok(self.extend_from_slice(slice))
    }

    fn finalize(self) -> Result<String, Self::FinalizeError> {
        Ok(String::from_utf8(self)?)
    }
}

impl Deserializable for String {
    type Deserializer = Vec<u8>;

    fn deserializer() -> Self::Deserializer {
        Self::Deserializer::new()
    }
}
