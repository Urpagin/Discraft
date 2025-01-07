use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "Discraft")]
#[command(author = "Urpagin")]
#[command(version = "0.1")]
#[command(about = "Play a Minecraft server through Discord", long_about = None)]
pub struct Args {
    #[command(subcommand)]
    pub mode: Mode,
}

#[derive(Subcommand, PartialEq, Clone)]
pub enum Mode {
    /// Run as the server-side
    Server {
        /// The Minecraft server address/IP
        #[arg(short, long)]
        address: String,

        /// The Minecraft server port
        #[arg(short, long, default_value_t = 25565)]
        port: u16,

        /// The Discord bot token
        #[arg(short, long)]
        token: String,

        /// The Discord guild ID
        #[arg(short, long)]
        guild_id: u64,
    },

    /// Run as the client-side
    Client {
        /// The Discord bot token
        #[arg(short, long)]
        token: String,

        /// The Discord guild ID
        #[arg(short, long)]
        guild_id: u64,
    },
}

/// Returns a usable args struct
pub fn parse() -> Args {
    Args::parse()
}
