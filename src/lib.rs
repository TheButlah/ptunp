use std::time::Duration;

use bytes::Bytes;
use color_eyre::{Result, eyre::Context};
use futures::{Sink, SinkExt, Stream, StreamExt, TryStream, TryStreamExt};
use tokio::{io::AsyncReadExt as _, task::JoinHandle};
use tokio_util::sync::CancellationToken;
use tun::AsyncDevice;

pub struct PTunP {
    cancel: CancellationToken,
    task_handle: tokio::task::JoinHandle<Result<()>>,
}

impl PTunP {
    pub fn spawn() -> Result<Self> {
        let cancel = CancellationToken::new();

        let mut config = tun::Configuration::default();
        config
            .address((10, 0, 0, 9))
            .netmask((255, 255, 255, 0))
            .destination((10, 0, 0, 1))
            .up();

        #[cfg(target_os = "linux")]
        config.platform_config(|config| {
            // requiring root privilege to acquire complete functions
            config.ensure_root_privileges(true);
        });

        let device = tun::create_as_async(&config)?;
        let framed = device
            .into_framed()
            .map(|vec| vec.map(Bytes::from))
            .with(|bytes: Bytes| {
                let result: Result<Vec<u8>, std::io::Error> = Ok(Vec::from(bytes));
                std::future::ready(result)
            });

        let task_handle = tokio::task::spawn(task(cancel.child_token(), framed));

        Ok(Self {
            cancel,
            task_handle,
        })
    }

    pub async fn join(self) -> Result<()> {
        self.task_handle.await.wrap_err("task panicked")?
    }
}

async fn task(
    cancel: CancellationToken,
    tun_device: impl TryStream<Ok = Bytes> + Sink<Bytes>,
) -> Result<()> {
    tokio::time::sleep(Duration::from_secs(10)).await;
    Ok(())
}
