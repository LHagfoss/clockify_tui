pub mod api;
mod config;
pub mod tui;

use clap::{Parser, Subcommand};
use std::process;

#[derive(Parser)]
#[command(name = "clockify")]
#[command(about = "A TUI client for Clockify time tracking", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Save your Clockify API Key
    Auth {
        /// The API token from Clockify Profile settings
        #[arg(short, long)]
        token: String,
    },
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Auth { token }) => {
            let mut cfg = match config::load() {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("Error loading config: {}", e);
                    process::exit(1);
                }
            };
            cfg.api_key = Some(token.trim().to_string());
            if let Err(e) = config::save(&cfg) {
                eprintln!("Error saving config: {}", e);
                process::exit(1);
            }
            println!("Successfully saved Clockify API Key!");
        }
        None => {
            let cfg = match config::load() {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("Error loading config: {}", e);
                    process::exit(1);
                }
            };

            if cfg.api_key.is_none() {
                eprintln!("Error: No Clockify API Key registered.");
                eprintln!(
                    "Please register your API Key first using: clockify auth --token <YOUR_TOKEN>"
                );
                process::exit(1);
            }

            let api_key = cfg.api_key.unwrap();
            tui::run(api_key);
        }
    }
}
