use crate::serialization::{Deserializable, Deserializer, Serializable, Serializer};

#[derive(Debug)]
pub enum OptionSerializer<'s, T: Serializable + 's> {
    Some(T::Serializer<'s>),
    None,
}

impl<T: Serializable> Serializer for OptionSerializer<'_, T> {
    fn fill(&mut self, buf: &mut [u8]) -> Option<usize> {
        match self {
            OptionSerializer::None => {
                buf[0] = 0;
                Some(1)
            }
            OptionSerializer::Some(t) => {
                buf[0] = 1;
                t.fill(&mut buf[1..]).map(|t| t + 1)
            }
        }
    }
}

impl<T: Serializable> Serializable for Option<T> {
    type Serializer<'s> = OptionSerializer<'s, T> where Self: 's;

    fn serializer(&self) -> Self::Serializer<'_> {
        match self {
            None => OptionSerializer::None,
            Some(t) => OptionSerializer::Some(t.serializer())
        }
    }
}

#[derive(Debug)]
pub enum OptionDeserializer<T: Deserializable> {
    Uninit,
    NoneInit,
    SomeInit(T::Deserializer),
}

impl<T: Deserializable> Deserializer<Option<T>> for OptionDeserializer<T> {
    type UpdateError = Option<<T::Deserializer as Deserializer<T>>::UpdateError>;
    type FinalizeError = Option<<T::Deserializer as Deserializer<T>>::FinalizeError>;

    fn update(&mut self, slice: &[u8]) -> Result<(), Self::UpdateError> {
        match (self, slice) {
            (t @ Self::Uninit, [0]) => Ok(*t = Self::NoneInit),
            (t @ Self::Uninit, [1, tail @ ..]) => {
                let mut d = T::deserializer();
                d.update(tail)?;
                *t = Self::SomeInit(d);
                Ok(())
            }
            (Self::SomeInit(t), slice) => Ok(t.update(slice)?),
            _ => Err(None)
        }
    }

    fn finalize(self) -> Result<Option<T>, Self::FinalizeError> {
        match self {
            OptionDeserializer::Uninit => Err(None),
            OptionDeserializer::NoneInit => Ok(None),
            OptionDeserializer::SomeInit(t) => Ok(Some(t.finalize()?))
        }
    }
}

impl<T: Deserializable> Deserializable for Option<T> {
    type Deserializer = OptionDeserializer<T>;

    fn deserializer() -> Self::Deserializer {
        Self::Deserializer::Uninit
    }
}
