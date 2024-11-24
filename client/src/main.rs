use std::error::Error;
use std::io;
use tokio::io::AsyncWriteExt;
use tokio::net::{TcpListener, TcpStream};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    println!("Hey!");

    let _ = listen().await;

    Ok(())
}

async fn listen() -> io::Result<()> {
    let listener = TcpListener::bind("127.0.0.1:25565").await?;

    loop {
        let (socket, var) = listener.accept().await?;
        println!("Got socket: {socket:#?} and var: {var:#?}");
    }
}
