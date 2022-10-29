#![feature(iter_array_chunks)]

use std::{
    collections::HashMap,
    io::Write,
};
use chat::client::connect;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
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
        match value.as_str() {
            "y" => Some(true),
            "n" => Some(false),
            _ => None,
        }
    }

    let conn = connect(
        map.remove("address").unwrap_or_else(|| read_line("ip")),
        map.remove("name").unwrap_or_else(|| read_line("name")),
        map.remove("password").unwrap_or_else(|| read_line("password")).into_bytes(),
        map.remove("new").map_or(None, check_bool).unwrap_or_else(|| check_bool(read_line("new? [y\\n]")).unwrap()),
        |e| println!("{}", e),
    );

    for [user, message] in std::io::stdin().lines().map_while(|line| {
        let line = line.unwrap().trim_end().to_owned();
        if line.is_empty() { None } else { Some(line) }
    }).array_chunks::<2>() {
        conn.send(user, message)?;
    }
    conn.shutdown().await
}