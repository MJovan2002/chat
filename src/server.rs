use std::{
    collections::hash_map::Entry,
    collections::HashMap,
};
use std::error::Error;
use std::fmt::{Debug, Display, Formatter};

use anyhow::anyhow;
use tokio::{
    net::{
        TcpListener,
        TcpStream,
        ToSocketAddrs,
    },
    sync::{
        mpsc::{
            self,
            UnboundedReceiver,
            UnboundedSender,
        },
        oneshot,
    },
    task,
};
use void::Void;

use crate::{
    spawn_and_handle,
    stream::BlockStream,
    db::{
        DataBase,
        DatabaseEvent,
        Password,
        User,
    },
    logger::Logger,
};

pub async fn accept_loop<Addrs: ToSocketAddrs, Db: DataBase, L: Logger + Clone + Send + Sync + 'static>(addr: Addrs, db: Db, mut shutdown_receiver: UnboundedReceiver<Void>, logger: L) -> anyhow::Result<()> {
    let listener = TcpListener::bind(addr).await?;

    logger.info("server started");

    let (broker_sender, broker_receiver) = mpsc::unbounded_channel();
    let broker_handle = task::spawn(broker_loop(broker_receiver, logger.clone()));
    let (database_sender, database_receiver) = mpsc::unbounded_channel();
    let database_handle = task::spawn(database_loop(db, database_receiver));

    while let Some(Ok((stream, address))) = tokio::select! {
        _ = async {
            if let Some(void) = shutdown_receiver.recv().await {
                match void {}
            }
        } => None,
        accepted = listener.accept() => Some(accepted),
    } {
        logger.info(&format!("Accepting from: {}", address));
        spawn_and_handle(connection_loop(broker_sender.clone(), database_sender.clone(), stream, logger.clone()), logger.clone());
    }
    drop(database_sender);
    drop(broker_sender);
    let (a, b) = tokio::join!(broker_handle, database_handle);

    logger.info("server stopped");

    match (a, b) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(e), Ok(())) => Err(e.into()),
        (Ok(()), Err(e)) => Err(e.into()),
        (Err(e0), Err(e1)) => {
            struct Pair<A, B>(A, B);

            impl<A: Debug, B: Debug> Debug for Pair<A, B> {
                fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
                    write!(f, "{:?}", (&self.0, &self.1))
                }
            }
            impl<A: Debug, B: Debug> Display for Pair<A, B> {
                fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
                    write!(f, "{:?}", (&self.0, &self.1))
                }
            }
            impl<A: Debug, B: Debug> Error for Pair<A, B> {}

            Err(Pair(e0, e1).into())
        }
    }
}

async fn connection_loop<L: Logger + Clone + Send + 'static>(broker_sender: UnboundedSender<Event>, database_sender: UnboundedSender<DatabaseEvent>, stream: TcpStream, logger: L) -> anyhow::Result<()> {
    let (reader, writer) = stream.into_split();
    let stream = BlockStream::new(writer, reader).await?;

    async fn log_in<L: Logger + Clone + Send + 'static>(database_sender: UnboundedSender<DatabaseEvent>, stream: &BlockStream, logger: L) -> anyhow::Result<User> {
        let mut writer = stream.get_writer().await;
        let mut reader = stream.get_reader().await;
        let new_user_block = match reader.read_block().await {
            Ok(block) if block.len() == 1 && block[0] < 2 => Ok(block[0] == 0),
            Err(error) => Err(error),
            _ => Err(anyhow!("invalid block")),
        };
        let new_user = match new_user_block {
            Ok(t) => {
                writer.write_block(&[1]).await?;
                t
            }
            Err(error) => {
                writer.write_block(&[0]).await?;
                writer.write_block(error.to_string().as_bytes()).await?;
                return Err(error);
            }
        };

        match if new_user {
            let name = match reader.read_block().await.map(String::from_utf8)? {
                Ok(t) => {
                    writer.write_block(&[1]).await?;
                    t
                }
                Err(error) => {
                    writer.write_block(&[0]).await?;
                    writer.write_block(error.to_string().as_bytes()).await?;
                    return Err(error.into());
                }
            };

            let password = match reader.read_block().await.map(Password::new)? {
                Ok(t) => {
                    writer.write_block(&[1]).await?;
                    t
                }
                Err(error) => {
                    writer.write_block(&[0]).await?;
                    writer.write_block(error.to_string().as_bytes()).await?;
                    return Err(error.into());
                }
            };

            let (s, r) = oneshot::channel();
            database_sender.send(DatabaseEvent::CreateUser {
                name,
                password,
                channel: s,
            }).unwrap();
            r
        } else {
            let name = match reader.read_block().await.map(String::from_utf8)? {
                Ok(t) => {
                    writer.write_block(&[1]).await?;
                    t
                }
                Err(error) => {
                    writer.write_block(&[0]).await?;
                    writer.write_block(error.to_string().as_bytes()).await?;
                    return Err(error.into());
                }
            };

            let password = reader.read_block().await?;
            let (s, r) = oneshot::channel();
            database_sender.send(DatabaseEvent::LogIn {
                name,
                password,
                channel: s,
            }).unwrap();
            r
        }.await? {
            None => {
                writer.write_block(&[0]).await?;
                writer.write_block(b"invalid credentials").await?;
                Err(anyhow!("invalid credentials"))
            }
            Some(t) => {
                writer.write_block(&[1]).await?;
                Ok(t)
            }
        }
    }

    let me = log_in(database_sender.clone(), &stream, logger.clone()).await?;

    let (_shutdown_sender, shutdown_receiver) = mpsc::unbounded_channel::<Void>();
    broker_sender.send(Event::NewPeer {
        user: me.clone(),
        stream: stream.clone(),
        shutdown: shutdown_receiver,
    }).unwrap();

    loop {
        let mut writer = stream.get_writer().await;
        let mut reader = stream.get_reader().await;
        let user_block = reader.read_block().await?;
        if user_block.len() == 0 {
            break;
        }
        let message_block = reader.read_block().await?;
        let (name, message) = match (String::from_utf8(user_block), String::from_utf8(message_block)) {
            (Ok(a), Ok(b)) => {
                writer.write_block(&[1]).await?;
                (a, b)
            }
            _ => {
                writer.write_block(&[0]).await?;
                writer.write_block(b"invalid utf8").await?;
                continue;
            }
        };

        let (s, r) = oneshot::channel();
        database_sender.send(DatabaseEvent::GetUser {
            name,
            channel: s,
        }).unwrap();
        let user = match r.await.unwrap() {
            None => {
                writer.write_block(&[0]).await?;
                writer.write_block(b"user not found").await?;
                continue;
            }
            Some(t) => {
                writer.write_block(&[1]).await?;
                t
            }
        };
        broker_sender.send(Event::Message {
            from: me.clone(),
            to: user,
            msg: message,
        }).unwrap();
    }

    Ok(())
}

async fn connection_writer_loop(
    messages: &mut UnboundedReceiver<String>,
    stream: BlockStream,
    mut shutdown: UnboundedReceiver<Void>,
) -> anyhow::Result<()> {
    while let Some(msg) = tokio::select! {
        msg = messages.recv() => msg,
        void = shutdown.recv() => void.map(Into::into)
    } {
        stream.get_writer().await.write_block(msg.as_bytes()).await?
    }
    Ok(())
}

enum Event {
    NewPeer {
        user: User,
        stream: BlockStream,
        shutdown: UnboundedReceiver<Void>,
    },
    Message {
        from: User,
        to: User,
        msg: String,
    },
}

impl Debug for Event {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Event::NewPeer { user, .. } => f.debug_struct("Event::NewPeer")
                .field("user", user)
                .finish(),
            Event::Message { from, to, msg } => f.debug_struct("Event::Message")
                .field("from", from)
                .field("to", to)
                .field("msg", msg)
                .finish()
        }
    }
}

async fn database_loop<Db: DataBase>(mut db: Db, mut events: UnboundedReceiver<DatabaseEvent>) {
    while let Some(event) = events.recv().await {
        match event {
            DatabaseEvent::LogIn { name, password, channel } => channel.send(db.log_in(name, &password)).unwrap(),
            DatabaseEvent::CreateUser { name, password, channel } => channel.send(db.create_user(name, password)).unwrap(),
            DatabaseEvent::GetUser { name, channel } => channel.send(db.user_from_username(&name)).unwrap(),
        }
    }
}

async fn broker_loop<L: Logger + Clone + Send + 'static>(mut events: UnboundedReceiver<Event>, logger: L) {
    let (disconnect_sender, mut disconnect_receiver) = mpsc::unbounded_channel::<((User, usize), UnboundedReceiver<String>)>();
    let mut peers: HashMap<User, (usize, HashMap<usize, UnboundedSender<String>>)> = HashMap::new();
    loop {
        let event = tokio::select! {
            event = events.recv() => match event {
                None => break,
                Some(event) => event,
            },
            disconnect = disconnect_receiver.recv() => {
                let ((name, counter), _pending_messages) = disconnect.unwrap();
                match peers.entry(name) {
                    std::collections::hash_map::Entry::Occupied(mut entry)=> {
                        assert!(entry.get_mut().1.remove(&counter).is_some());
                        if entry.get().1.len() == 0{
                            entry.remove();
                        }
                    }
                    std::collections::hash_map::Entry::Vacant(_) => unreachable!(""),
                }
                continue;
            },
        };
        match event {
            Event::Message { from, to, msg } => {
                if let Some((_, peers)) = peers.get_mut(&to) {
                    for (_, peer) in peers {
                        let msg = format!("from {}: {}", from, msg);
                        peer.send(msg).unwrap()
                    }
                }
            }
            Event::NewPeer { user, stream, shutdown } => {
                let (counter, mut client_receiver) = match peers.entry(user.clone()) {
                    Entry::Occupied(mut entry) => {
                        let (client_sender, client_receiver) = mpsc::unbounded_channel();
                        let (counter, map) = entry.get_mut();
                        let t = *counter;
                        map.insert(t, client_sender);
                        *counter += 1;
                        (t, client_receiver)
                    }
                    Entry::Vacant(entry) => {
                        let (client_sender, client_receiver) = mpsc::unbounded_channel();
                        entry.insert((1, HashMap::from([(0, client_sender)])));
                        (0, client_receiver)
                    }
                };
                let disconnect_sender = disconnect_sender.clone();
                spawn_and_handle(async move {
                    let res = connection_writer_loop(&mut client_receiver, stream, shutdown).await;
                    disconnect_sender.send(((user, counter), client_receiver)).unwrap();
                    res
                }, logger.clone());
            }
        }
    }
    drop(peers);
    drop(disconnect_sender);
    while let Some((_name, _pending_messages)) = disconnect_receiver.recv().await {}
}
