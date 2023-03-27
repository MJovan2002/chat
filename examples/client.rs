#![feature(iter_array_chunks)]

use std::{
    collections::HashMap,
    io::Write,
};

use tokio::{
    io::{AsyncBufReadExt, BufReader},
    task::JoinError,
};

use chat::{
    client::{self, InitError, LoopError},
    serialization::Deserializable,
};

#[derive(Debug)]
enum Error<M: Deserializable> {
    Init(InitError),
    Loop(LoopError<M>),
    Join(JoinError),
}

impl<M: Deserializable> From<InitError> for Error<M> {
    fn from(value: InitError) -> Self {
        Self::Init(value)
    }
}

impl<M: Deserializable> From<LoopError<M>> for Error<M> {
    fn from(value: LoopError<M>) -> Self {
        Self::Loop(value)
    }
}

impl<M: Deserializable> From<JoinError> for Error<M> {
    fn from(value: JoinError) -> Self {
        Self::Join(value)
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Error<String>> {
    let mut map = std::env::args()
        .skip(1)
        .array_chunks::<2>()
        .map(|[key, value]| (key, value)).collect::<HashMap<_, _>>();

    fn read_line(prompt: &str) -> String {
        print!("{prompt}> ");
        std::io::stdout().flush().unwrap();
        let mut word = String::new();
        std::io::stdin().read_line(&mut word).unwrap();
        word.trim_end().to_owned()
    }

    fn check_bool(value: String) -> Option<bool> {
        match value.to_lowercase().as_str() {
            "y" | "" => Some(true),
            "n" => Some(false),
            _ => None,
        }
    }

    let conn = client::Builder::new::<String>()
        .name(map.remove("name").unwrap_or_else(|| read_line("name")))
        .addr(map.remove("address").unwrap_or_else(|| read_line("address")))
        .password(map.remove("password").unwrap_or_else(|| read_line("password")).into_bytes())
        .first(map.remove("new").map_or(None, check_bool).unwrap_or_else(|| check_bool(read_line("new? [Y/n]")).unwrap()))
        .writer(|user, message| println!("{user}> {message}"))
        .connect::<1024>().await?;

    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    loop {
        let Ok(Some(name)) = lines.next_line().await else { break; };
        if name.len() == 0 { break; }
        let Ok(Some(message)) = lines.next_line().await else { break; };
        let Ok(()) = conn.send((name, message)) else { break; };
    }
    Ok(conn.shutdown().await??)
}