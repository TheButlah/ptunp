use clap::Parser;
use color_eyre::Result;
use tracing::{debug, info};

#[derive(Debug, Parser)]
#[clap(about, version)]
struct Args {}

#[tokio::main]
async fn main() -> Result<()> {
    color_eyre::install()?;
    tracing_subscriber::fmt::init();
    let _args = Args::parse();

    info!("starting");

    let ptunp = ptunp::PTunP::spawn()?;
    ptunp.join().await?;

    Ok(())
}
