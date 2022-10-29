use std::{
    collections::{
        hash_map::Entry,
        HashMap,
    },
    fs::File,
    io::{
        Read,
        Seek,
        SeekFrom,
        Write,
    },
    path::Path,
};
use std::fs::OpenOptions;
use std::net::Ipv4Addr;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::mpsc;
use chat::server::accept_loop;

use chat::db::{
    DataBase,
    Password,
    User,
};
use chat::logger::{FileLogger, StdioLogger};

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
        for (_, (name, password)) in std::mem::replace(&mut self.users, HashMap::new()) {
            let name = name.into_bytes();
            self.file.write_all(&[name.len() as u8]).unwrap();
            self.file.write_all(&name).unwrap();
            let password = password.into_string().into_bytes();
            self.file.write_all(&[password.len() as u8]).unwrap();
            self.file.write_all(&password).unwrap();
        }
    }
}

impl DataBase for InMemoryDB {
    fn log_in(&mut self, name: String, password: &[u8]) -> Option<User> {
        let user_data = self.users.get(&name)?;
        user_data.1.verify(password).ok().map(|_| user_data.0.clone())
    }

    fn create_user(&mut self, name: String, password: Password) -> Option<User> {
        match self.users.entry(name.clone()) {
            Entry::Occupied(_) => None,
            Entry::Vacant(e) => {
                let user = User::new(name);
                e.insert((user.clone(), password));
                Some(user)
            }
        }
    }

    fn user_from_username(&mut self, name: &str) -> Option<User> {
        self.users.get(name).map(|it| it.0.clone())
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let (ss, sr) = mpsc::unbounded_channel();
    let handle = tokio::task::spawn(accept_loop(
        (Ipv4Addr::from([0, 0, 0, 0]), 5000),
        InMemoryDB::new("chat.txt"),
        sr,
        StdioLogger,
    ));
    std::io::stdin().read_line(&mut String::new()).unwrap();
    drop(ss);
    handle.await??;
    Ok(())
}
