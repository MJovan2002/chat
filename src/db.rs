use std::fmt::{Debug, Display, Formatter};
use std::future::Future;
use std::string::FromUtf8Error;

use futures::channel::oneshot;

use pbkdf2::{
    password_hash::{
        PasswordHash,
        PasswordHasher,
        PasswordVerifier,
        SaltString,
    },
    Pbkdf2,
};
use pbkdf2::password_hash::Error;

use rand_core1::OsRng;
use tokio::sync::mpsc::UnboundedReceiver;
use crate::serialization::{Deserializable, Deserializer, Serializable};

pub enum DatabaseEvent {
    LogIn {
        name: String,
        password: Vec<u8>,
        channel: oneshot::Sender<User>,
    },
    CreateUser {
        name: String,
        password: Password,
        channel: oneshot::Sender<User>,
    },
    GetUser {
        name: String,
        channel: oneshot::Sender<User>,
    },
}

fn _f() {
    fn is_send<T: Send + 'static>() {}

    is_send::<&mut UnboundedReceiver<DatabaseEvent>>();
    is_send::<DatabaseError>();
}

#[derive(Debug)]
pub enum DatabaseError {
    LogIn,
    CreateUser,
    GetUser,
    Channel(User),
}

impl DatabaseEvent {
    pub async fn execute<DB: DataBase>(self, db: &mut DB) -> Result<(), DatabaseError> {
        match self {
            DatabaseEvent::LogIn { name, password, channel } => channel.send(db.log_in(name, &password).await.ok_or(DatabaseError::LogIn)?),
            DatabaseEvent::CreateUser { name, password, channel } => channel.send(db.create_user(name, password).await.ok_or(DatabaseError::CreateUser)?),
            DatabaseEvent::GetUser { name, channel } => channel.send(db.user_from_username(&name).await.ok_or(DatabaseError::GetUser)?),
        }.map_err(|e| DatabaseError::Channel(e))
    }
}

impl Debug for DatabaseEvent {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            DatabaseEvent::LogIn { name, .. } => f.debug_struct("LogIn").field("name", name).finish(),
            DatabaseEvent::CreateUser { name, .. } => f.debug_struct("CreateUser").field("name", name).finish(),
            DatabaseEvent::GetUser { name, .. } => f.debug_struct("GetUser").field("name", name).finish(),
        }
    }
}

pub trait DataBase: Sync + 'static {
    type LogInFuture: Future<Output=Option<User>> + Send;
    type CreateUserFuture: Future<Output=Option<User>> + Send;
    type UserFromUsernameFuture: Future<Output=Option<User>> + Send;

    fn log_in(&mut self, name: String, password: &[u8]) -> Self::LogInFuture;

    fn create_user(&mut self, name: String, password: Password) -> Self::CreateUserFuture;

    fn user_from_username(&mut self, name: &str) -> Self::UserFromUsernameFuture;
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct User {
    name: String,
}

impl User {
    pub fn new(name: String) -> Self {
        Self { name }
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.name.into_bytes()
    }

    pub fn as_bytes(&self) -> &[u8] {
        self.name.as_bytes()
    }
}

impl Display for User {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        Display::fmt(&self.name, f)
    }
}

impl Serializable for User {
    type Serializer<'s> = <String as Serializable>::Serializer<'s>;

    fn serializer(&self) -> Self::Serializer<'_> {
        self.name.serializer()
    }
}

pub struct UserDeserializer {
    buf: Vec<u8>,
}

impl Deserializer<User> for UserDeserializer {
    type UpdateError = !;
    type FinalizeError = FromUtf8Error;

    fn update(&mut self, slice: &[u8]) -> Result<(), Self::UpdateError> {
        Ok(self.buf.extend_from_slice(slice))
    }

    fn finalize(self) -> Result<User, Self::FinalizeError> {
        Ok(User::new(String::from_utf8(self.buf)?))
    }
}

impl Deserializable for User {
    type Deserializer = UserDeserializer;

    fn deserializer() -> Self::Deserializer {
        todo!()
    }
}

pub struct Password {
    hash: String,
}

impl Password {
    pub(crate) fn new(block: &[u8]) -> Self {
        let salt = SaltString::generate(&mut OsRng);
        let hash = Pbkdf2.hash_password(block, &salt).unwrap().to_string();
        Self { hash }
    }

    pub fn verify(&self, password: &[u8]) -> Result<(), Error> {
        let hash = PasswordHash::new(&self.hash).unwrap();
        Pbkdf2.verify_password(password, &hash)
    }

    pub fn from(hash: String) -> Self {
        Self { hash }
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.hash.into_bytes()
    }

    pub fn as_bytes(&self) -> &[u8] {
        self.hash.as_bytes()
    }
}
