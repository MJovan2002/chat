use std::fmt::{Debug, Display, Formatter};

use anyhow::anyhow;
use pbkdf2::{
    password_hash::{
        PasswordHash,
        PasswordHasher,
        PasswordVerifier,
        SaltString,
    },
    Pbkdf2,
};
use rand_core1::OsRng;
use tokio::sync::oneshot;

pub enum DatabaseEvent {
    LogIn {
        name: String,
        password: Vec<u8>,
        channel: oneshot::Sender<Option<User>>,
    },
    CreateUser {
        name: String,
        password: Password,
        channel: oneshot::Sender<Option<User>>,
    },
    GetUser {
        name: String,
        channel: oneshot::Sender<Option<User>>,
    },
}

impl Debug for DatabaseEvent {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            DatabaseEvent::LogIn { name, .. } => f.debug_struct("DatabaseEvent::LogIn").field("name", name).finish(),
            DatabaseEvent::CreateUser { name, .. } => f.debug_struct("DatabaseEvent::CreateUser").field("name", name).finish(),
            DatabaseEvent::GetUser { name, .. } => f.debug_struct("DatabaseEvent::GetUser").field("name", name).finish(),
        }
    }
}

pub trait DataBase: Sync + Send + 'static {
    fn log_in(&mut self, name: String, password: &[u8]) -> Option<User>;

    fn create_user(&mut self, name: String, password: Password) -> Option<User>;

    fn user_from_username(&mut self, name: &str) -> Option<User>;
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct User {
    name: String,
}

impl Display for User {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        Display::fmt(&self.name, f)
    }
}

impl User {
    pub fn new(name: String) -> Self {
        Self {
            name,
        }
    }

    pub(crate) fn from(block: Vec<u8>) -> anyhow::Result<Self> {
        Ok(Self::new(String::from_utf8(block)?))
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.name.into_bytes()
    }
}

#[derive(Clone)]
pub struct Password {
    hash: String,
}

impl Display for Password {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        Display::fmt(&self.hash, f)
    }
}

impl Password {
    pub(crate) fn new(block: Vec<u8>) -> anyhow::Result<Self> {
        let salt = SaltString::generate(&mut OsRng);
        let hash = Pbkdf2.hash_password(&block, &salt).map_err(|e| anyhow!("{}", e.to_string()))?.to_string();
        Ok(Self {
            hash,
        })
    }

    pub fn verify(&self, password: &[u8]) -> anyhow::Result<()> {
        let hash = PasswordHash::new(&self.hash).map_err(|e| anyhow!("{}", e.to_string()))?;
        Ok(Pbkdf2.verify_password(password, &hash).map_err(|e| anyhow!("{}", e.to_string()))?)
    }

    pub fn from(hash: String) -> Self {
        Self {
            hash,
        }
    }

    pub fn into_string(self) -> String {
        self.hash
    }
}
