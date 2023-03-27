use crate::serialization::{Deserializable, Deserializer, Serializable, Serializer};

pub enum ResultSerializer<'s, T: Serializable + 's, E: Serializable + 's> {
    Ok(T::Serializer<'s>),
    Err(E::Serializer<'s>),
}

impl<T: Serializable, E: Serializable> Serializer for ResultSerializer<'_, T, E> {
    fn fill(&mut self, buf: &mut [u8]) -> Option<usize> {
        match self {
            ResultSerializer::Ok(t) => {
                buf[0] = 1;
                t.fill(&mut buf[1..])
            }
            ResultSerializer::Err(t) => {
                buf[0] = 0;
                t.fill(&mut buf[1..])
            }
        }.map(|t| t + 1)
    }
}

impl<T: Serializable, E: Serializable> Serializable for Result<T, E> {
    type Serializer<'s> = ResultSerializer<'s, T, E> where Self: 's;

    fn serializer(&self) -> Self::Serializer<'_> {
        match self {
            Ok(t) => ResultSerializer::Ok(t.serializer()),
            Err(t) => ResultSerializer::Err(t.serializer()),
        }
    }
}

#[derive(Debug)]
pub enum ResultDeserializer<T: Deserializable, E: Deserializable> {
    Uninit,
    OkInit(T::Deserializer),
    ErrInit(E::Deserializer),
}

#[derive(Debug)]
pub enum ResultDeserializationError<T, E> {
    OkError(T),
    ErrError(E),
    BlockError,
}

impl<T: Deserializable, E: Deserializable> Deserializer<Result<T, E>> for ResultDeserializer<T, E> {
    type UpdateError = ResultDeserializationError<<T::Deserializer as Deserializer<T>>::UpdateError, <E::Deserializer as Deserializer<E>>::UpdateError>;
    type FinalizeError = ResultDeserializationError<<T::Deserializer as Deserializer<T>>::FinalizeError, <E::Deserializer as Deserializer<E>>::FinalizeError>;

    fn update(&mut self, slice: &[u8]) -> Result<(), Self::UpdateError> {
        match (self, slice) {
            (t @ Self::Uninit, [0, tail @ ..]) => {
                let mut d = E::deserializer();
                d.update(tail).map_err(|e| ResultDeserializationError::ErrError(e))?;
                Ok(*t = Self::ErrInit(d))
            }
            (t @ Self::Uninit, [1, tail @ ..]) => {
                let mut d = T::deserializer();
                d.update(tail).map_err(|e| ResultDeserializationError::OkError(e))?;
                Ok(*t = Self::OkInit(d))
            }
            (Self::ErrInit(t), slice) => Ok(t.update(slice).map_err(|e| ResultDeserializationError::ErrError(e))?),
            (Self::OkInit(t), slice) => Ok(t.update(slice).map_err(|e| ResultDeserializationError::OkError(e))?),
            _ => Err(ResultDeserializationError::BlockError)
        }
    }

    fn finalize(self) -> Result<Result<T, E>, Self::FinalizeError> {
        match self {
            ResultDeserializer::Uninit => Err(ResultDeserializationError::BlockError),
            ResultDeserializer::OkInit(t) => Ok(Ok(t.finalize().map_err(|e| ResultDeserializationError::OkError(e))?)),
            ResultDeserializer::ErrInit(t) => Ok(Err(t.finalize().map_err(|e| ResultDeserializationError::ErrError(e))?)),
        }
    }
}

impl<T: Deserializable, E: Deserializable> Deserializable for Result<T, E> {
    type Deserializer = ResultDeserializer<T, E>;

    fn deserializer() -> Self::Deserializer {
        Self::Deserializer::Uninit
    }
}
