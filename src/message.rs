use anyhow;

use crate::db::User;

pub trait MessageType: Sized {
    fn into_bytes(self) -> Vec<u8>;

    fn from(bytes: Vec<u8>) -> anyhow::Result<Self>;
}

impl MessageType for String {
    fn into_bytes(self) -> Vec<u8> {
        self.into_bytes()
    }

    fn from(bytes: Vec<u8>) -> anyhow::Result<Self> {
        Ok(Self::from_utf8(bytes)?)
    }
}

impl MessageType for User {
    fn into_bytes(self) -> Vec<u8> {
        self.into_bytes()
    }

    fn from(bytes: Vec<u8>) -> anyhow::Result<Self> {
        Self::from(bytes)
    }
}

#[derive(Debug)]
pub struct Message<T: MessageType> {
    meta: T,
    text: String,
}

impl<T: MessageType> Message<T> {
    pub fn new(user: T, text: String) -> Self {
        Self {
            meta: user,
            text,
        }
    }

    pub fn into_bytes(self) -> Vec<u8> {
        let mut t = vec![0];
        t.append(&mut self.meta.into_bytes());
        t[0] = (t.len() - 1) as u8;
        t.append(&mut self.text.into_bytes());
        t
    }

    pub fn from(mut data: Vec<u8>) -> anyhow::Result<Self> {
        let mut user = data.split_off(1);
        let text = user.split_off(data[0] as usize);
        Ok(Self {
            meta: T::from(user)?,
            text: String::from_utf8(text)?,
        })
    }

    pub fn get_text(&self) -> &str {
        &self.text
    }
}

impl Message<String> {
    pub fn get_username(&self) -> &str {
        &self.meta
    }

    pub fn invert(self, user: User) -> Message<User> {
        Message::new(user, self.text)
    }
}

impl Message<User> {
    pub fn get_user(&self) -> &User {
        &self.meta
    }
}
