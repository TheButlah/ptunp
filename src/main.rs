use clap::Parser;
use color_eyre::Result;
use tokio_util::sync::CancellationToken;
use tracing::info;

#[derive(Debug, Parser)]
#[clap(about, version)]
struct Args {}

#[tokio::main]
async fn main() -> Result<()> {
    color_eyre::install()?;
    tracing_subscriber::fmt::init();
    let _args = Args::parse();

    info!("starting");

    let cancel = CancellationToken::new();

    let ctrlc_cancel = cancel.clone();
    tokio::task::spawn(async move {
        let _cancel_guard = ctrlc_cancel.drop_guard();
        let _ = tokio::signal::ctrl_c().await;
        info!("ctrl-c detected, shutting down");
    });
    let ptunp = ptunp::PTunP::builder()
        .with_cancel(cancel.child_token())
        .build()
        .await?;

    ptunp.join().await
}
