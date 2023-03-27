use std::fmt::{Debug, Formatter};

use tokio::{
    io::{
        self,
        AsyncReadExt,
        AsyncWriteExt,
    },
    net::TcpStream,
};

use rand_core2::{
    OsRng,
    RngCore,
};

use aes::{
    Aes256,
    cipher::KeyInit,
};

use aes_gcm_siv::{aead::{
    AeadMutInPlace,
    Error,
    heapless,
}, AesGcmSiv, Nonce};

use x25519_dalek::{
    EphemeralSecret,
    PublicKey,
};

use crate::serialization::{Deserializable, Deserializer, Serializable, Serializer};

pub struct BlockStream<const N: usize = 1024, S: AsyncWriteExt + AsyncReadExt + Unpin + Send + 'static = TcpStream> {
    stream: S,
    aes: AesGcmSiv<Aes256>,
}

impl<S: AsyncWriteExt + AsyncReadExt + Unpin + Send + 'static, const N: usize> BlockStream<N, S> where [(); N + 16]: {
    pub async fn new(mut stream: S) -> Result<Self, io::Error> {
        let my_secret = EphemeralSecret::new(OsRng);

        let my_public = PublicKey::from(&my_secret);
        stream.write_all(&my_public.to_bytes()).await?;

        let mut public = [0; 32];
        stream.read_exact(&mut public).await?;
        let public = PublicKey::from(public);

        let shared = my_secret.diffie_hellman(&public);

        let key = shared.to_bytes().into();
        let aes = aes::Aes256::new(&key).into();

        Ok(Self {
            stream,
            aes,
        })
    }

    pub async fn write_block<B: Serializable>(&mut self, block: B) -> Result<(), WriteError> {
        let mut serializer = block.serializer();
        let mut buf = heapless::Vec::<_, { N + 16 }>::from_slice(&[0; N]).unwrap();
        loop {
            let finished = match serializer.fill(&mut buf[8..N]) {
                None => {
                    buf[..8].fill(0);
                    false
                }
                Some(len) => {
                    let t = (len + 1).to_be_bytes();
                    buf.iter_mut().zip(t.into_iter()).for_each(|(a, b)| *a = b);
                    true
                }
            };
            let mut nonce = [0; 12];
            OsRng.fill_bytes(&mut nonce);
            let nonce = Nonce::from(nonce);

            self.aes.encrypt_in_place(&nonce, b"", &mut buf).map_err(|e| WriteError::EncryptError(e))?;
            self.stream.write_all(&nonce).await.map_err(|e| WriteError::NetworkError(e))?;
            self.stream.write_all(&buf).await.map_err(|e| WriteError::NetworkError(e))?;

            if finished {
                break Ok(());
            }
        }
    }

    pub async fn read_block<T: Deserializable>(&mut self) -> Result<T, ReadError<T>> where [(); N + 16]: {
        let mut output = T::deserializer();
        loop {
            let mut nonce = [0; 12];
            self.stream.read_exact(&mut nonce).await.map_err(|e| ReadError::NetworkError(e))?;
            let nonce = Nonce::from(nonce);

            let mut block = [0; N + 16];
            self.stream.read_exact(&mut block).await.map_err(|e| ReadError::NetworkError(e))?;

            let mut decrypted = heapless::Vec::<_, { N + 16 }>::from_slice(&block).unwrap();
            self.aes.decrypt_in_place(&nonce, b"", &mut decrypted).map_err(|e| ReadError::DecryptError(e))?;
            let n = usize::from_be_bytes(decrypted[..8].try_into().unwrap());
            if n == 0 {
                output.update(&decrypted[8..]).map_err(|e| ReadError::UpdateError(e))?;
            } else {
                output.update(&decrypted[8..7 + n]).map_err(|e| ReadError::UpdateError(e))?;
                break;
            }
        }
        Ok(output.finalize().map_err(|e| ReadError::FinalizeError(e))?)
    }
}

#[derive(Debug)]
pub enum WriteError {
    NetworkError(io::Error),
    EncryptError(Error),
}

pub enum ReadError<T: Deserializable> {
    NetworkError(io::Error),
    DecryptError(Error),
    UpdateError(<T::Deserializer as Deserializer<T>>::UpdateError),
    FinalizeError(<T::Deserializer as Deserializer<T>>::FinalizeError),
}

impl<T: Deserializable> Debug for ReadError<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        struct S<'s, T>(&'s T);

        impl<T> Debug for S<'_, T> {
            default fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
                write!(f, "<>")
            }
        }

        impl<T: Debug> Debug for S<'_, T> {
            fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
                write!(f, "{:?}", self.0)
            }
        }

        match self {
            ReadError::NetworkError(e) => write!(f, "NetworkError({e:?})"),
            ReadError::DecryptError(e) => write!(f, "DecryptError({e:?})"),
            ReadError::UpdateError(e) => write!(f, "UpdateError({:?})", S(e)),
            ReadError::FinalizeError(e) => write!(f, "FinalizeError({:?})", S(e)),
        }
    }
}
