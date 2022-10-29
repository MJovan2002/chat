use std::{
    iter,
    sync::Arc,
};

use aes::Aes256;
use aes::cipher::KeyInit;
use aes::cipher::Key;

use aes_gcm_siv::{
    aead::{
        Aead,
        AeadInPlace,
        heapless,
    },
    Aes256GcmSiv,
    AesGcmSiv,
    // Key,
    Nonce,
};

use anyhow::anyhow;

use tokio::{
    sync::Mutex,
    io::{
        AsyncWriteExt,
        AsyncReadExt,
    },
    net::tcp::{OwnedReadHalf, OwnedWriteHalf},
};

use rand_core2::{
    OsRng,
    RngCore,
};
use tokio::sync::MutexGuard;

use x25519_dalek::{
    EphemeralSecret,
    PublicKey,
};

pub struct BlockStream<const N: usize = 1024, W: AsyncWriteExt + Send + Unpin + 'static = OwnedWriteHalf, R: AsyncReadExt + Send + Unpin + 'static = OwnedReadHalf> {
    writer: Arc<Mutex<W>>,
    reader: Arc<Mutex<R>>,
    aes: Arc<AesGcmSiv<Aes256>>,
}

impl<W: AsyncWriteExt + Send + Unpin + 'static, R: AsyncReadExt + Send + Unpin + 'static, const N: usize> BlockStream<N, W, R> {
    pub async fn new(mut writer: W, mut reader: R) -> anyhow::Result<Self> {
        let my_secret = EphemeralSecret::new(OsRng);
        let my_public = PublicKey::from(&my_secret);
        writer.write_all(&my_public.to_bytes()).await?;
        let mut public = [0; 32];
        reader.read_exact(&mut public).await?;
        let public = PublicKey::from(public);
        let shared = my_secret.diffie_hellman(&public);
        let key = shared.to_bytes().into();
        let aes = aes::Aes256::new(&key).into();
        Ok(Self {
            writer: Arc::new(Mutex::new(writer)),
            reader: Arc::new(Mutex::new(reader)),
            aes: Arc::new(aes),
        })
    }

    pub async fn get_writer(&self) -> WriteGuard<'_, W, N> {
        WriteGuard {
            inner: self.writer.lock().await,
            aes: self.aes.clone(),
        }
    }

    pub async fn get_reader(&self) -> ReadGuard<'_, R, N> {
        ReadGuard {
            inner: self.reader.lock().await,
            aes: self.aes.clone(),
        }
    }
}

impl<W: AsyncWriteExt + Send + Sync + Unpin + 'static, R: AsyncReadExt + Send + Sync + Unpin + 'static, const N: usize> Clone for BlockStream<N, W, R> {
    fn clone(&self) -> Self {
        Self {
            writer: self.writer.clone(),
            reader: self.reader.clone(),
            aes: self.aes.clone(),
        }
    }
}

pub struct WriteGuard<'s, W, const N: usize> {
    inner: MutexGuard<'s, W>,
    aes: Arc<AesGcmSiv<Aes256>>,
}

impl<'s, W: AsyncWriteExt + Unpin + Send + 'static, const N: usize> WriteGuard<'s, W, N> {
    pub async fn write_block(&mut self, block: &[u8]) -> anyhow::Result<()> {
        struct Merge<I0, I1> {
            iter0: I0,
            iter1: I1,
            pos: usize,
            choose: fn(usize) -> bool,
        }

        impl<T, I0: Iterator<Item=T>, I1: Iterator<Item=T>> Iterator for Merge<I0, I1> {
            type Item = T;

            fn next(&mut self) -> Option<Self::Item> {
                if (self.choose)({
                    let t = self.pos;
                    self.pos += 1;
                    t
                }) {
                    self.iter0.next()
                } else {
                    self.iter1.next()
                }
            }
        }

        trait Mergeable: Sized + Iterator {
            fn merge<I: Iterator<Item=Self::Item>>(self, other: I, choose: fn(usize) -> bool) -> Merge<Self, I>;
        }

        impl<I: Iterator> Mergeable for I {
            fn merge<O: Iterator<Item=Self::Item>>(self, other: O, choose: fn(usize) -> bool) -> Merge<Self, O> {
                Merge {
                    iter0: self,
                    iter1: other,
                    pos: 0,
                    choose,
                }
            }
        }

        fn get_last_block_size<const N: usize>(len: usize) -> usize {
            if len == 0 {
                0
            } else {
                (len - 1) % (N - 8) + 1
            }
        }

        let num_of_blocks = block.len().div_ceil(N - 8).max(1);
        let last_block_size = (get_last_block_size::<N>(block.len()) + 1).to_be_bytes();

        let chunks = block
            .iter()
            .copied()
            .chain(iter::repeat(0).take(N - 7))
            .merge(iter::repeat(0).take(8 * (num_of_blocks - 1)).chain(last_block_size.into_iter()), |pos| pos % N >= 8)
            .array_chunks::<N>();
        for block in chunks {
            let mut nonce = [0; 12];
            OsRng.fill_bytes(&mut nonce);
            let nonce = Nonce::from(nonce);

            let block = self.aes.encrypt(&nonce, block.as_slice()).map_err(|_| anyhow!("todo")).unwrap();
            self.inner.write_all(&nonce).await?;
            self.inner.write_all(&block).await?;
        }

        Ok(())
    }
}

pub struct ReadGuard<'s, R, const N: usize> {
    inner: MutexGuard<'s, R>,
    aes: Arc<AesGcmSiv<Aes256>>,
}

impl<'s, R: AsyncReadExt + Unpin + Send + 'static, const N: usize> ReadGuard<'_, R, N> {
    pub async fn read_block(&mut self) -> anyhow::Result<Vec<u8>> where [(); N + 16]: {
        let mut reader = &mut *self.inner;
        let mut output = vec![];

        loop {
            let mut nonce = [0; 12];
            reader.read_exact(&mut nonce).await?;
            let nonce = Nonce::from(nonce);

            let mut block = [0; N + 16];
            reader.read_exact(&mut block).await?;

            let mut decrypted = heapless::Vec::<_, { N + 16 }>::from_slice(&block).unwrap();
            self.aes.decrypt_in_place(&nonce, b"", &mut decrypted).map_err(|_| anyhow!("todo"))?;
            let n = usize::from_be_bytes(decrypted[..8].try_into().unwrap());
            if n == 0 {
                output.extend_from_slice(&decrypted[8..]);
            } else {
                output.extend_from_slice(&decrypted[8..7 + n]);
                break;
            }
        }
        Ok(output)
    }
}

#[cfg(test)]
mod tests {
    use std::io::Error;
    use std::pin::Pin;
    use std::task::{Context, Poll};
    use std::sync::Arc;
    use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
    use tokio::sync::{Mutex, MutexGuard};
    use crate::stream::BlockStream;

    #[tokio::test]
    async fn read_write() {
        struct Reader {
            inner: Arc<Mutex<Vec<u8>>>,
            pos: usize,
        }

        impl AsyncRead for Reader {
            fn poll_read(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &mut ReadBuf<'_>) -> Poll<std::io::Result<()>> {
                let count = match Pin::new(&mut self.inner).lock().poll(cx) {
                    Poll::Ready(lock) => {
                        let count = std::cmp::min(buf.remaining(), lock.len() - self.pos);
                        buf.put_slice(&lock[self.pos..self.pos + count]);
                        count
                    }
                    Poll::Pending => return Poll::Pending
                };
                self.pos += count;
                Poll::Ready(Ok(()))
            }
        }

        struct Writer {
            inner: Arc<Mutex<Vec<u8>>>,
        }

        impl AsyncWrite for Writer {
            fn poll_write(mut self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> Poll<Result<usize, Error>> {
                match Pin::new(&mut self.inner).lock().poll(cx) {
                    Poll::Ready(mut lock) => {
                        lock.extend_from_slice(buf);
                        Poll::Ready(Ok(buf.len()))
                    }
                    Poll::Pending => Poll::Pending,
                }
            }

            fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Error>> {
                Poll::Ready(Ok(()))
            }

            fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Error>> {
                Poll::Ready(Ok(()))
            }
        }

        let buf = Arc::new(Mutex::new(Vec::new()));
        let reader = Reader {
            inner: buf.clone(),
            pos: 0,
        };
        let writer = Writer {
            inner: buf,
        };
        let stream = BlockStream::new(writer, reader).await.unwrap();
        let t = b"hello";
        stream.get_writer().await.write_block(t).await.unwrap();
        assert_eq!(stream.get_reader().await.read_block().await.unwrap(), t);
    }
}
