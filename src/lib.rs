use bytes::Bytes;
use color_eyre::{Result, eyre::Context};
use futures::{Sink, SinkExt, StreamExt, TryStream};
use tokio_util::sync::CancellationToken;
use tracing::debug;

pub struct PTunP {
    _cancel: CancellationToken,
    task_handle: tokio::task::JoinHandle<Result<()>>,
}

impl PTunP {
    pub fn spawn(cancel: CancellationToken) -> Result<Self> {
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
            _cancel: cancel,
            task_handle,
        })
    }

    pub async fn join(self) -> Result<()> {
        self.task_handle.await.wrap_err("task panicked")?
    }
}

async fn task(
    cancel: CancellationToken,
    _tun_device: impl TryStream<Ok = Bytes> + Sink<Bytes>,
) -> Result<()> {
    debug!("starting task");
    let pending_fut = std::future::pending();
    let cancel_fut = cancel.cancelled();
    tokio::select! {
        _ = pending_fut => unreachable!(),
        _ = cancel_fut => {
            debug!("ctrlc detected, cancelling task");
        },
    }

    Ok(())
}
