use std::{
    fs::File,
    io::{Error, Write},
    pin::Pin,
    task::{Context, Poll},
};

use tokio::{
    net::{
        TcpStream,
        ToSocketAddrs,
        tcp::OwnedWriteHalf,
    },
    task,
    io::AsyncWrite,
    sync::mpsc,
};

use crate::
// message::Message,
stream::BlockStream
// server::db::User
;

struct CopyWriter {
    writer: OwnedWriteHalf,
    file: File,
}

impl AsyncWrite for CopyWriter {
    fn poll_write(mut self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> Poll<Result<usize, Error>> {
        self.file.write_all(buf).unwrap();
        Pin::new(&mut self.writer).poll_write(cx, buf)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Error>> {
        Pin::new(&mut self.writer).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Error>> {
        Pin::new(&mut self.writer).poll_shutdown(cx)
    }
}

pub fn connect<A: ToSocketAddrs + Send + 'static, F: FnMut(String) + Send + 'static>(addr: A, name: String, password: Vec<u8>, new_user: bool, mut write: F) -> ClientHandle {
    let (sender, mut receiver) = mpsc::unbounded_channel::<(String, String)>();
    let handle = task::spawn(async move {
        // let writer = CopyWriter { writer, file: std::fs::File::create("/home/jovan/Desktop/dump").unwrap() };
        let (reader, writer) = TcpStream::connect(addr).await?.into_split();
        let stream = BlockStream::<1024>::new(writer, reader).await?;
        let mut reader = stream.get_reader().await;
        let mut writer = stream.get_writer().await;
        writer.write_block(&[if new_user { 0 } else { 1 }]).await?;
        assert_eq!(reader.read_block().await?, [1]);
        writer.write_block(name.as_bytes()).await?;
        assert_eq!(reader.read_block().await?, [1]);
        writer.write_block(&password).await?;
        if new_user {
            assert_eq!(reader.read_block().await?, [1]);
        }
        drop(reader);
        drop(writer);
        tokio::select! {
            res = async {
                while let Some((user, message)) = receiver.recv().await {
                    let mut reader = stream.get_reader().await;
                    let mut writer = stream.get_writer().await;
                    writer.write_block(user.as_bytes()).await?;
                    writer.write_block(message.as_bytes()).await?;
                    assert_eq!(reader.read_block().await?, [1]);
                }
                stream.get_writer().await.write_block(&[]).await?;
                Ok(())
            } => res,
            res = async {
                while let Ok(block) = stream.get_reader().read_block().await {
                    write(String::from_utf8(block)?)
                }
                Ok(())
            } => res,
        }
    });
    ClientHandle { sender, handle }
}

pub struct ClientHandle {
    sender: mpsc::UnboundedSender<(String, String)>,
    handle: task::JoinHandle<anyhow::Result<()>>,
}

impl ClientHandle {
    pub fn send(&self, user: String, message: String) -> anyhow::Result<()> {
        self.sender.send((user, message))?;
        Ok(())
    }

    pub async fn shutdown(self) -> anyhow::Result<()> {
        let ClientHandle { sender, handle } = self;
        drop(sender);
        handle.await?
    }
}
