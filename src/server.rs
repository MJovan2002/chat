use std::{
    collections::{
        hash_map::Entry,
        HashMap,
    },
    fmt::Debug,
    sync::Arc,
};

use futures::{
    channel::oneshot::{self, Canceled},
    future,
    FutureExt,
};

use tokio::{
    io,
    net::{TcpListener, TcpStream, ToSocketAddrs},
    sync::mpsc::{
        self,
        error::SendError,
        UnboundedReceiver,
        UnboundedSender,
    },
    task::JoinError,
};

use crate::{
    db::{
        DataBase,
        DatabaseError,
        DatabaseEvent,
        Password,
        User,
    },
    handle::Handle,
    logger::Logger,
    message::Message,
    serialization::{
        Deserializable,
        Deserializer,
    },
    stream::{
        BlockStream,
        ReadError,
        WriteError,
    },
    Uninitialized,
};

pub struct Builder<A, DB, L> {
    addr: A,
    db: DB,
    logger: L,
}

impl Builder<Uninitialized, Uninitialized, Uninitialized> {
    pub fn new() -> Self {
        Self {
            addr: Uninitialized,
            db: Uninitialized,
            logger: Uninitialized,
        }
    }
}

impl<DB, L> Builder<Uninitialized, DB, L> {
    pub fn addr<A: ToSocketAddrs + Send + 'static>(self, addr: A) -> Builder<A, DB, L> {
        let Self { addr: _, db, logger } = self;
        Builder { addr, db, logger }
    }
}

impl<A, L> Builder<A, Uninitialized, L> {
    pub fn db<DB: DataBase + Send + 'static>(self, db: DB) -> Builder<A, DB, L> {
        let Self { addr, db: _, logger } = self;
        Builder { addr, db, logger }
    }
}

impl<A, DB> Builder<A, DB, Uninitialized> {
    pub fn logger<L: Logger + Send + 'static>(self, logger: L) -> Builder<A, DB, L> {
        let Self { addr, db, logger: _ } = self;
        Builder { addr, db, logger }
    }
}

#[derive(Clone)]
pub struct Sink;

impl Logger for Sink {
    fn fatal<S: Debug>(&self, _: S) {}

    fn error<S: Debug>(&self, _: S) {}

    fn warn<S: Debug>(&self, _: S) {}

    fn info<S: Debug>(&self, _: S) {}
}

pub trait IntoLogger {
    type Logger: Logger + Send + 'static;

    fn into_logger(self) -> Self::Logger;
}

impl IntoLogger for Uninitialized {
    type Logger = Sink;

    fn into_logger(self) -> Self::Logger {
        Sink
    }
}

impl<L: Logger + Send + 'static> IntoLogger for L {
    type Logger = L;

    fn into_logger(self) -> Self::Logger {
        self
    }
}

impl<
    A: ToSocketAddrs + Send + 'static,
    DB: DataBase + Send + 'static,
    L: IntoLogger,
> Builder<A, DB, L> {
    pub fn serve<
        const N: usize,
        M: for<'s> Message<Serializer<'s>: Send, Deserializer: Send> + Send + Sync + 'static,
    >(self) -> Handle<!, Result<(), ServerError<M>>> where [(); N + 16]:, <<M as Deserializable>::Deserializer as Deserializer<M>>::UpdateError: Send, <<M as Deserializable>::Deserializer as Deserializer<M>>::FinalizeError: Send {
        let Self { addr, db, logger } = self;
        let logger = logger.into_logger();

        Handle::new(|mut shutdown_receiver| async move {
            let logger_clone = logger.clone();
            let message_loop = Handle::<TcpStream, Result<(), ServerError<M>>>::new(|mut receiver| async move {
                // todo: remove arc
                let db_loop = Arc::new(Handle::new(|receiver| db_loop(db, receiver)));

                let mut handles = HashMap::<User, Vec<Handle<(User, M), Result<(), ConnectionLoopError<M>>>>>::new();
                let (message_sender, mut message_receiver) = mpsc::unbounded_channel::<((User, User), M)>();

                loop {
                    futures::select! {
                        m = message_receiver.recv().fuse() => {
                            let((from, to), message) = m.ok_or(ServerError::MessageReceiver)?;
                            if let Some(handle) = handles.get_mut(&from) {
                                handle.retain_mut(|handle| handle.send((to.clone(), message.clone())).is_ok())
                            }
                        }
                        m = receiver.recv().fuse() => {
                            let Some(stream) = m else { break };
                            let(user, handle) = match connection_loop(stream, db_loop.clone(), message_sender.clone()).await {
                                Ok(t) => t,
                                Err(e) => {
                                    logger_clone.error(e);
                                    continue
                                }
                            };
                            match handles.entry(user) {
                                Entry::Occupied(mut e) => { e.get_mut().push(handle) }
                                Entry::Vacant(e) => { e.insert(vec![handle]); }
                            }
                        }
                    }
                }

                future::join_all(handles.into_values()
                    .flatten()
                    .map(Handle::shutdown))
                    .await
                    .into_iter()
                    .try_for_each(|t| match t {
                        Ok(Ok(())) => Ok(()),
                        Ok(Err(e)) => Err(ServerError::Connection(e)),
                        Err(e) => Err(ServerError::Join(e)),
                    })?;
                Arc::into_inner(db_loop).ok_or(ServerError::DbArcDrop)?.shutdown().await??;
                Ok(())
            });


            let listener = TcpListener::bind(addr).await?;
            while let Some(stream) = futures::select! {
                _ = shutdown_receiver.recv().fuse() => None,
                accept = listener.accept().fuse() => {
                    let (stream, _) = accept?;
                    Some(stream)
                }
            } {
                if let Err(e) = message_loop.send(stream) {
                    logger.error(e)
                }
            }

            message_loop.shutdown().await??;
            Ok(())
        })
    }
}


#[derive(Debug)]
pub enum ServerError<M: Deserializable> {
    Connection(ConnectionLoopError<M>),
    Join(JoinError),
    MessageReceiver,
    DbArcDrop,
    DatabaseLoop(DatabaseError),
    Io(io::Error),
}

impl<M: Deserializable> From<ConnectionLoopError<M>> for ServerError<M> {
    fn from(value: ConnectionLoopError<M>) -> Self {
        Self::Connection(value)
    }
}

impl<M: Deserializable> From<JoinError> for ServerError<M> {
    fn from(value: JoinError) -> Self {
        Self::Join(value)
    }
}

impl<M: Deserializable> From<DatabaseError> for ServerError<M> {
    fn from(value: DatabaseError) -> Self {
        Self::DatabaseLoop(value)
    }
}

impl<M: Deserializable> From<io::Error> for ServerError<M> {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

#[derive(Debug)]
enum ConnectionInitError {
    StreamCreation(io::Error),
    LogIn(LogInError, Option<WriteError>),
    Write(WriteError),
}

impl From<io::Error> for ConnectionInitError {
    fn from(value: io::Error) -> Self {
        Self::StreamCreation(value)
    }
}

impl From<WriteError> for ConnectionInitError {
    fn from(value: WriteError) -> Self {
        Self::Write(value)
    }
}

#[derive(Debug)]
enum LogInError {
    First(ReadRespondError<[u8; 1], ()>),
    Name(ReadRespondError<String, ()>),
    Password(ReadRespondError<Vec<u8>, !>),
    ChannelSend(SendError<DatabaseEvent>),
    ChannelReceive,
}

impl From<ReadRespondError<[u8; 1], ()>> for LogInError {
    fn from(value: ReadRespondError<[u8; 1], ()>) -> Self {
        Self::First(value)
    }
}

impl From<ReadRespondError<String, ()>> for LogInError {
    fn from(value: ReadRespondError<String, ()>) -> Self {
        Self::Name(value)
    }
}

impl From<ReadRespondError<Vec<u8>, !>> for LogInError {
    fn from(value: ReadRespondError<Vec<u8>, !>) -> Self {
        Self::Password(value)
    }
}

impl From<SendError<DatabaseEvent>> for LogInError {
    fn from(value: SendError<DatabaseEvent>) -> Self {
        Self::ChannelSend(value)
    }
}

impl From<Canceled> for LogInError {
    fn from(_: Canceled) -> Self {
        Self::ChannelReceive
    }
}

#[derive(Debug)]
enum ReadRespondError<T: Deserializable, E> {
    Read(ReadError<T>),
    Transform(E),
    Write(WriteError),
}

struct TransformError<T>(T);

impl<T: Deserializable, E> From<TransformError<E>> for ReadRespondError<T, E> {
    fn from(TransformError(value): TransformError<E>) -> Self {
        Self::Transform(value)
    }
}

impl<T: Deserializable, E> From<ReadError<T>> for ReadRespondError<T, E> {
    fn from(value: ReadError<T>) -> Self {
        Self::Read(value)
    }
}

impl<T: Deserializable, E> From<WriteError> for ReadRespondError<T, E> {
    fn from(value: WriteError) -> Self {
        Self::Write(value)
    }
}

#[derive(Debug)]
pub enum ConnectionLoopError<M: Deserializable> {
    Message(MessageReadError<M>),
    Send(SendError<((User, User), M)>),
    Write(WriteError),
}

impl<M: Deserializable> From<MessageReadError<M>> for ConnectionLoopError<M> {
    fn from(value: MessageReadError<M>) -> Self {
        Self::Message(value)
    }
}

impl<M: Deserializable> From<SendError<((User, User), M)>> for ConnectionLoopError<M> {
    fn from(value: SendError<((User, User), M)>) -> Self {
        Self::Send(value)
    }
}

impl<M: Deserializable> From<WriteError> for ConnectionLoopError<M> {
    fn from(value: WriteError) -> Self {
        Self::Write(value)
    }
}

#[derive(Debug)]
pub enum MessageReadError<M: Deserializable> {
    UserRead(ReadError<Option<String>>),
    Database(SendError<DatabaseEvent>),
    User,
    MessageRead(ReadError<M>),
}

struct ReadUser(ReadError<Option<String>>);

impl<M: Deserializable> From<ReadUser> for MessageReadError<M> {
    fn from(ReadUser(value): ReadUser) -> Self {
        Self::UserRead(value)
    }
}

impl<M: Deserializable> From<SendError<DatabaseEvent>> for MessageReadError<M> {
    fn from(value: SendError<DatabaseEvent>) -> Self {
        Self::Database(value)
    }
}

impl<M: Deserializable> From<Canceled> for MessageReadError<M> {
    fn from(_: Canceled) -> Self {
        Self::User
    }
}

struct ReadMessage<M: Deserializable>(ReadError<M>);

impl<M: Deserializable> From<ReadMessage<M>> for MessageReadError<M> {
    fn from(ReadMessage(value): ReadMessage<M>) -> Self {
        Self::MessageRead(value)
    }
}

async fn connection_loop<M: for<'s> Message<Serializer<'s>: Send, Deserializer: Send> + Send + Sync + 'static, const N: usize>(
    stream: TcpStream,
    db_sender: Arc<Handle<DatabaseEvent, Result<(), DatabaseError>>>,
    message_sender: UnboundedSender<((User, User), M)>,
) -> Result<(User, Handle<(User, M), Result<(), ConnectionLoopError<M>>>), ConnectionInitError> where [(); N + 16]:, <<M as Deserializable>::Deserializer as Deserializer<M>>::UpdateError: Send, <<M as Deserializable>::Deserializer as Deserializer<M>>::FinalizeError: Send {
    async fn log_in<const N: usize>(
        stream: &mut BlockStream<N>,
        db: &Handle<DatabaseEvent, Result<(), DatabaseError>>,
    ) -> Result<User, LogInError> where [(); N + 16]: {
        async fn read_respond<T: Deserializable, const N: usize, U, E, F: FnOnce(T) -> Result<U, E>>(
            stream: &mut BlockStream<N>,
            f: F,
        ) -> Result<U, ReadRespondError<T, E>> where [(); N + 16]: {
            let t = f(stream.read_block::<T>().await?);
            let r = match &t {
                Ok(_) => Ok(()),
                Err(_) => Err(()),
            };
            stream.write_block(&r).await?;
            Ok(t.map_err(|e| TransformError(e))?)
        }

        let first = read_respond(stream, |[t]: [u8; 1]| match t {
            t @ (0 | 1) => Ok(t == 1),
            _ => Err(())
        }).await?;

        let name = read_respond(stream, |t| Ok(t)).await?;

        let (sender, receiver) = oneshot::channel();
        let event = if first {
            DatabaseEvent::CreateUser {
                name,
                password: read_respond(stream, |block: Vec<u8>| Ok(Password::new(&block))).await?,
                channel: sender,
            }
        } else {
            DatabaseEvent::LogIn {
                name,
                password: read_respond(stream, |block| Ok(block)).await?,
                channel: sender,
            }
        };

        db.send(event)?;

        Ok(receiver.await?)
    }

    let mut stream = BlockStream::<N>::new(stream).await?;

    let user = match log_in(&mut stream, &*db_sender).await {
        Ok(user) => Ok(user),
        Err(e) => Err(ConnectionInitError::LogIn(
            e,
            {
                let _ = stream.write_block(Err::<(), &str>("invalid credentials")).await;
                if let Err(e) = Ok(()) {
                    Some(e)
                } else {
                    None
                }
            },
        )),
    }?;
    stream.write_block(Ok::<(), &str>(())).await?;

    let user_clone = user.clone();

    let handle = Handle::<(User, M), _>::new(|mut receiver| async move {
        async fn read_message<const N: usize, M: Message>(
            stream: &mut BlockStream<N>,
            db_sender: &Handle<DatabaseEvent, Result<(), DatabaseError>>,
        ) -> Result<Option<(User, M)>, MessageReadError<M>> where [(); N + 16]: {
            let Some(user) = stream.read_block::<Option<String>>().await.map_err(|e| ReadUser(e))? else {
                return Ok(None);
            };
            let (sender, receiver) = oneshot::channel();
            db_sender.send(DatabaseEvent::GetUser {
                name: user,
                channel: sender,
            })?;
            let user = receiver.await?;
            let message = stream.read_block::<M>().await.map_err(|e| ReadMessage(e))?;
            Ok(Some((user, message)))
        }

        loop {
            futures::select! {
                m = read_message(&mut stream, &*db_sender).fuse() => {
                    let Some((from, message)) = m? else {
                        break Ok(())
                    };
                    message_sender.send(((from, user_clone.clone()), message))?;
                },
                m = receiver.recv().fuse() => {
                    let Some((user, message)) = m else {
                        break Ok(())
                    };
                    stream.write_block(user.into_bytes()).await?;
                    stream.write_block(message).await?;
                }
            }
        }
    });

    Ok((user, handle))
}

async fn db_loop<DB: DataBase + Send + 'static>(mut db: DB, mut event_receiver: UnboundedReceiver<DatabaseEvent>) -> Result<(), DatabaseError> {
    while let Some(event) = event_receiver.recv().await {
        event.execute(&mut db).await?;
    }
    Ok(())
}
