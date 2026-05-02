mod cli;
mod crawler;
mod headless;
mod network;
mod parsers;
mod processors;
mod utils;

#[tokio::main]
async fn main() {
    if let Err(e) = cli::run_cli().await {
        eprintln!("{}", e);
        std::process::exit(1);
    }
}
