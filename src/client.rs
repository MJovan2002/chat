use std::marker::PhantomData;
use futures::FutureExt;

use tokio::{
    io,
    net::{
        TcpStream,
        ToSocketAddrs,
    },
    sync::mpsc,
};

use crate::{
    handle::Handle,
    message::Message,
    stream::{
        BlockStream,
        ReadError,
        WriteError,
    },
    serialization::{
        Serializable,
        Deserializable,
        Deserializer,
    },
};
use crate::Uninitialized;

pub struct Builder<A, N, P, F, W, M> {
    addr: A,
    name: N,
    password: P,
    first: F,
    writer: W,
    _marker: PhantomData<M>,
}

impl Builder<Uninitialized, Uninitialized, Uninitialized, Uninitialized, Uninitialized, Uninitialized> {
    pub fn new<M>() -> Builder<Uninitialized, Uninitialized, Uninitialized, Uninitialized, Uninitialized, M> {
        Builder {
            addr: Uninitialized,
            name: Uninitialized,
            password: Uninitialized,
            first: Uninitialized,
            writer: Uninitialized,
            _marker: PhantomData,
        }
    }
}

impl<N, P, F, W, M> Builder<Uninitialized, N, P, F, W, M> {
    pub fn addr<A: ToSocketAddrs>(self, addr: A) -> Builder<A, N, P, F, W, M> {
        let Self {
            addr: _, name, password, first, writer, _marker
        } = self;
        Builder {
            addr,
            name,
            password,
            first,
            writer,
            _marker,
        }
    }
}

impl<A, P, F, W, M> Builder<A, Uninitialized, P, F, W, M> {
    pub fn name(self, name: String) -> Builder<A, String, P, F, W, M> {
        let Self {
            addr, name: _, password, first, writer, _marker
        } = self;
        Builder {
            addr,
            name,
            password,
            first,
            writer,
            _marker,
        }
    }
}

impl<A, N, F, W, M> Builder<A, N, Uninitialized, F, W, M> {
    pub fn password(self, password: Vec<u8>) -> Builder<A, N, Vec<u8>, F, W, M> {
        let Self {
            addr, name, password: _, first, writer, _marker
        } = self;
        Builder {
            addr,
            name,
            password,
            first,
            writer,
            _marker,
        }
    }
}

impl<A, N, P, W, M> Builder<A, N, P, Uninitialized, W, M> {
    pub fn first(self, first: bool) -> Builder<A, N, P, bool, W, M> {
        let Self {
            addr, name, password, first: _, writer, _marker
        } = self;
        Builder {
            addr,
            name,
            password,
            first,
            writer,
            _marker,
        }
    }

    pub fn not_first(self) -> Builder<A, N, P, bool, W, M> {
        let Self {
            addr, name, password, first: _, writer, _marker
        } = self;
        Builder {
            addr,
            name,
            password,
            first: false,
            writer,
            _marker,
        }
    }
}

impl<A, N, P, F, M> Builder<A, N, P, F, Uninitialized, M> {
    pub fn writer<W: FnMut(String, M) + Send + 'static>(self, writer: W) -> Builder<A, N, P, F, W, M> {
        let Self {
            addr, name, password, first, writer: _, _marker
        } = self;
        Builder {
            addr,
            name,
            password,
            first,
            writer,
            _marker,
        }
    }
}

impl<A: ToSocketAddrs, W: FnMut(String, M) + Send + 'static, M: for<'s> Message<Serializer<'s>: Send, Deserializer: Send> + Send + Sync + 'static> Builder<A, String, Vec<u8>, bool, W, M> where <<M as Deserializable>::Deserializer as Deserializer<M>>::UpdateError: Send, <<M as Deserializable>::Deserializer as Deserializer<M>>::FinalizeError: Send {
    pub async fn connect<
        const N: usize,
    >(self) -> Result<Handle<(String, M), Result<(), LoopError<M>>>, InitError> where [(); N + 16]: {
        let Self { addr, name, password, first, mut writer, .. } = self;

        async fn write_react<const N: usize, T: Serializable>(stream: &mut BlockStream<N>, message: Option<T>) -> Result<(), InitError> where [(); N + 16]: {
            if let Some(message) = message {
                stream.write_block(message).await?;
            }
            Ok(stream.read_block::<Result<(), ()>>().await??)
        }

        let mut stream = BlockStream::<N>::new(TcpStream::connect(addr).await?).await?;

        write_react(&mut stream, Some([if first { 1 } else { 0 }].as_slice())).await?;
        write_react(&mut stream, Some(name)).await?;
        write_react(&mut stream, Some(password.as_slice())).await?;
        write_react(&mut stream, None::<()>).await?;

        Ok(Handle::new(|mut receiver: mpsc::UnboundedReceiver<(String, M)>| async move {
            loop {
                futures::select! {
                m = async {
                    let user = stream.read_block::<String>().await.map_err(|e| ReadUser(e))?;
                    let message = stream.read_block::<M>().await.map_err(|e| ReadMessage(e))?;
                    Ok::<_, LoopError<M>>((user, message))
                }.fuse() => {
                    let (user, message) = m?;
                    writer(user, message)
                },
                m = receiver.recv().fuse() => {
                    let Some((user, message)) = m else {
                        break
                    };
                    stream.write_block(Some(user)).await?;
                    stream.write_block(message).await?;
                }
            }
            }
            stream.write_block(None::<String>).await?;
            Ok(())
        }))
    }
}

#[derive(Debug)]
pub enum InitError {
    Write(WriteError),
    Read(ReadError<Result<(), ()>>),
    LogIn,
    Io(io::Error),
}

impl From<WriteError> for InitError {
    fn from(value: WriteError) -> Self {
        Self::Write(value)
    }
}

impl From<ReadError<Result<(), ()>>> for InitError {
    fn from(value: ReadError<Result<(), ()>>) -> Self {
        Self::Read(value)
    }
}

impl From<()> for InitError {
    fn from((): ()) -> Self {
        Self::LogIn
    }
}

impl From<io::Error> for InitError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

#[derive(Debug)]
pub enum LoopError<M: Deserializable> {
    ReadUser(ReadError<String>),
    ReadMessage(ReadError<M>),
    Write(WriteError),
}

impl<M: Deserializable> From<WriteError> for LoopError<M> {
    fn from(value: WriteError) -> Self {
        Self::Write(value)
    }
}

struct ReadMessage<M: Deserializable>(ReadError<M>);

impl<M: Deserializable> From<ReadMessage<M>> for LoopError<M> {
    fn from(ReadMessage(value): ReadMessage<M>) -> Self {
        Self::ReadMessage(value)
    }
}

struct ReadUser(ReadError<String>);

impl<M: Deserializable> From<ReadUser> for LoopError<M> {
    fn from(ReadUser(value): ReadUser) -> Self {
        Self::ReadUser(value)
    }
}
