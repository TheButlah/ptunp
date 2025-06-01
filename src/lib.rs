use std::net::Ipv4Addr;

use bytes::Bytes;
use color_eyre::{
    Result, Section,
    eyre::{Context, eyre},
};
use futures::{Sink, SinkExt, StreamExt, TryStream};
use tokio_util::sync::{CancellationToken, DropGuard};
use tracing::debug;

const ALPN: &str = "ptunp-v0";
// TODO: double check we can use 0,1 and not 255, 254
const OUR_IP: Ipv4Addr = Ipv4Addr::new(10, 0, 0, 0);
const THEIR_IP: Ipv4Addr = Ipv4Addr::new(10, 0, 0, 1);

pub struct PTunP {
    cancel: CancellationToken,
    _cancel_guard: DropGuard,
    task_handle: tokio::task::JoinHandle<Result<()>>,
}

pub struct PTunPBuilder {
    pub cancel: Option<CancellationToken>,
    pub tun_cfg: Option<tun::Configuration>,
}

impl PTunPBuilder {
    pub fn new() -> Self {
        Self {
            cancel: None,
            tun_cfg: None,
        }
    }

    pub async fn build(self) -> Result<PTunP> {
        PTunP::spawn(self).await
    }

    pub fn with_cancel(mut self, cancel: CancellationToken) -> Self {
        self.cancel = Some(cancel);
        self
    }

    pub fn with_tun_cfg(mut self, tun_cfg: tun::Configuration) -> Self {
        self.tun_cfg = Some(tun_cfg);
        self
    }
}

impl Default for PTunPBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl PTunP {
    pub fn builder() -> PTunPBuilder {
        PTunPBuilder::new()
    }

    async fn spawn(builder: PTunPBuilder) -> Result<Self> {
        let tun_cfg = builder.tun_cfg.unwrap_or_else(|| {
            let mut tun_cfg = tun::Configuration::default();
            tun_cfg
                .address(OUR_IP)
                // Use last bit only for ip address
                .netmask((255, 255, 255, u8::MAX << 1))
                .destination(THEIR_IP)
                .up();

            #[cfg(target_os = "linux")]
            tun_cfg.platform_config(|tun_cfg| {
                // requiring root privilege to acquire complete functions
                tun_cfg.ensure_root_privileges(true);
            });

            tun_cfg
        });
        let cancel = builder.cancel.unwrap_or_else(|| CancellationToken::new());
        let cancel_guard = cancel.clone().drop_guard();

        let device = tun::create_as_async(&tun_cfg)
            .wrap_err("failed to create tun network device")
            .with_suggestion(|| "try running as root on linux")?;

        // tun stream/sink uses Vec<u8>, this adapts it to use `Bytes`
        let framed = device
            .into_framed()
            .map(|vec| vec.map(Bytes::from))
            .with(|bytes: Bytes| {
                let result: Result<Vec<u8>, std::io::Error> = Ok(Vec::from(bytes));
                std::future::ready(result)
            });

        let endpoint = iroh::Endpoint::builder()
            .alpns(vec![String::from(ALPN).into()])
            .bind()
            .await
            .map_err(|err| eyre!(format!("{err}")))
            .wrap_err("failed to bind iroh endpoint")?;

        let task_handle = tokio::task::spawn(task(cancel.child_token(), framed, endpoint));

        Ok(Self {
            cancel,
            _cancel_guard: cancel_guard,
            task_handle,
        })
    }

    /// Wait for all tasks to finish and propagate any errors.
    ///
    /// This is preferable to silently discarding erros in `Drop`.
    pub async fn join(self) -> Result<()> {
        self.task_handle.await.wrap_err("task panicked")?
    }

    /// Cancel the tasks. You can also control cancellation by passing in a cancellation token
    /// into the [`PTunPBuilder::with_cancel()`].
    ///
    /// After cancelling, you probably want to call [`Self::join`].
    ///
    /// Note that Dropping `PTunP` also cancels it.
    pub fn cancel(&mut self) {
        self.cancel.cancel();
    }
}

async fn task(
    cancel: CancellationToken,
    _tun_device: impl TryStream<Ok = Bytes> + Sink<Bytes>,
    _endpoint: iroh::Endpoint,
) -> Result<()> {
    let _cancel_guard = cancel.clone().drop_guard();
    debug!("starting task");
    let pending_fut = std::future::pending();
    let cancel_fut = cancel.cancelled();
    tokio::select! {
        _ = pending_fut => unreachable!(),
        _ = cancel_fut => {
            debug!("cancelling task");
        },
    }

    Ok(())
}
