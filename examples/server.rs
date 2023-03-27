#![feature(type_alias_impl_trait)]

use std::{
    collections::{
        hash_map::Entry,
        HashMap,
    },
    fs::{
        File,
        OpenOptions,
    },
    io::{
        Read,
        Seek,
        SeekFrom,
        Write,
    },
    path::Path,
    future::Future,
};
use futures::future;

use tokio::{
    io,
    task::JoinError,
};

use chat::{
    server::{
        self,
        ServerError,
    },
    db::{
        DataBase,
        Password,
        User,
    },
    serialization::Deserializable,
    logger::StdioLogger,
};

struct InMemoryDB {
    users: HashMap<String, (User, Password)>,
    file: File,
}

impl InMemoryDB {
    pub fn new<P: AsRef<Path>>(path: P) -> Self {
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(path).unwrap();
        let mut users = HashMap::new();
        loop {
            let mut buf = [0];
            match file.read_exact(&mut buf) {
                Ok(_) => {}
                Err(_) => break,
            }
            let mut buf = vec![0; buf[0] as usize];
            file.read_exact(&mut buf).unwrap();
            let name = String::from_utf8(buf).unwrap();
            let mut buf = [0];
            file.read_exact(&mut buf).unwrap();
            let mut buf = vec![0; buf[0] as usize];
            file.read_exact(&mut buf).unwrap();
            let password = String::from_utf8(buf).unwrap();
            users.insert(name.clone(), (User::new(name), Password::from(password)));
        }
        Self {
            users,
            file,
        }
    }
}

impl Drop for InMemoryDB {
    fn drop(&mut self) {
        self.file.seek(SeekFrom::Start(0)).unwrap();
        for (_, (name, password)) in self.users.drain() {
            let name = name.as_bytes();
            self.file.write_all(&[name.len() as u8]).unwrap();
            self.file.write_all(&name).unwrap();
            let password = password.as_bytes();
            self.file.write_all(&[password.len() as u8]).unwrap();
            self.file.write_all(password).unwrap();
        }
    }
}

impl DataBase for InMemoryDB {
    type LogInFuture = impl Future<Output=Option<User>> + Send;
    type CreateUserFuture = impl Future<Output=Option<User>> + Send;
    type UserFromUsernameFuture = impl Future<Output=Option<User>> + Send;

    fn log_in(&mut self, name: String, password: &[u8]) -> Self::LogInFuture {
        fn log_in(db: &mut InMemoryDB, name: String, password: &[u8]) -> Option<User> {
            let user_data = db.users.get(&name)?;
            user_data.1.verify(password).ok().map(|()| user_data.0.clone())
        }

        future::ready(log_in(self, name, password))
    }

    fn create_user(&mut self, name: String, password: Password) -> Self::CreateUserFuture {
        future::ready(match self.users.entry(name.clone()) {
            Entry::Occupied(_) => None,
            Entry::Vacant(e) => {
                let user = User::new(name);
                e.insert((user.clone(), password));
                Some(user)
            }
        })
    }

    fn user_from_username(&mut self, name: &str) -> Self::UserFromUsernameFuture {
        future::ready(self.users.get(name).map(|it| it.0.clone()))
    }
}

#[derive(Debug)]
enum Error<M: Deserializable> {
    Cancel(io::Error),
    Join(JoinError),
    Server(ServerError<M>),
}

impl<M: Deserializable> From<io::Error> for Error<M> {
    fn from(value: io::Error) -> Self {
        Self::Cancel(value)
    }
}

impl<M: Deserializable> From<JoinError> for Error<M> {
    fn from(value: JoinError) -> Self {
        Self::Join(value)
    }
}

impl<M: Deserializable> From<ServerError<M>> for Error<M> {
    fn from(value: ServerError<M>) -> Self {
        Self::Server(value)
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Error<String>> {
    let handle = server::Builder::new()
        .addr(([0, 0, 0, 0], 5000))
        .db(InMemoryDB::new("chat.txt"))
        .logger(StdioLogger)
        .serve::<1024, String>();
    tokio::signal::ctrl_c().await?;
    // let _ = tokio::io::BufReader::new(tokio::io::stdin()).read_line(&mut String::new()).await;
    Ok(handle.shutdown().await??)
}
