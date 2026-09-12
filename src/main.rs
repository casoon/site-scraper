mod cli;
mod crawler;
mod headless;
mod network;
mod output;
mod parsers;
mod processors;
mod screenshot;
mod utils;

#[tokio::main]
async fn main() {
    if let Err(e) = cli::run_cli().await {
        output::print_error(&e);
        std::process::exit(1);
    }
}
