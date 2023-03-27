mod byte_vec;
mod option;
mod result;
mod never;
mod unit;
mod string;

pub trait Serializable {
    type Serializer<'s>: Serializer where Self: 's;

    fn serializer(&self) -> Self::Serializer<'_>;
}

pub trait Serializer {
    fn fill(&mut self, buf: &mut [u8]) -> Option<usize>;
}

pub trait Deserializable: Sized {
    type Deserializer: Deserializer<Self>;

    fn deserializer() -> Self::Deserializer;
}

pub trait Deserializer<T> {
    type UpdateError;
    type FinalizeError;

    fn update(&mut self, slice: &[u8]) -> Result<(), Self::UpdateError>;

    fn finalize(self) -> Result<T, Self::FinalizeError>;
}

impl<T> Serializable for &T where T: Serializable {
    type Serializer<'s> = <T as Serializable>::Serializer<'s> where Self: 's;

    fn serializer(&self) -> Self::Serializer<'_> {
        (*self).serializer()
    }
}

// impl<T, U:Deserializer<T>> Deserialize for T {
//     type Deserializer = U;
// }

pub struct Buf<'s> {
    buf: &'s [u8],
    pos: usize,
}

impl<'s> Buf<'s> {
    pub fn new(buf: &'s [u8]) -> Self {
        Self {
            buf,
            pos: 0,
        }
    }

    pub fn fill(&mut self, buf: &mut [u8]) -> Option<usize> {
        buf.iter_mut().zip(self.buf[self.pos..].iter().copied()).for_each(|(a, b)| *a = b);
        let pos = self.pos;
        self.pos += buf.len();
        self.buf.len().checked_sub(pos)
    }
}

impl Serializer for Buf<'_> {
    fn fill(&mut self, buf: &mut [u8]) -> Option<usize> {
        self.fill(buf)
    }
}
