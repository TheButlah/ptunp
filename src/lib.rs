pub mod auth;
mod iroh_protocol;

use std::{net::Ipv4Addr, sync::Arc};

use auth::{AuthStrategy, NoAuth};
use color_eyre::{
    Result, Section,
    eyre::{Context, eyre},
};
use tokio_util::sync::{CancellationToken, DropGuard};

const ALPN_PREFIX: &str = "ptunp/v0";
// TODO: double check we can use 0,1 and not 255, 254
const OUR_IP: Ipv4Addr = Ipv4Addr::new(10, 0, 0, 0);
const THEIR_IP: Ipv4Addr = Ipv4Addr::new(10, 0, 0, 1);

pub struct ServerBuilder<A = NoAuth> {
    pub cancel: Option<CancellationToken>,
    pub tun_cfg: Option<tun::Configuration>,
    // TODO: Eventually we should support *multiple* auth handlers registered at once
    pub auth_handler: A,
}

impl ServerBuilder {
    pub fn new() -> Self {
        Self {
            cancel: None,
            tun_cfg: None,
            auth_handler: NoAuth,
        }
    }
}

impl<A> ServerBuilder<A> {
    pub fn with_cancel(mut self, cancel: CancellationToken) -> Self {
        self.cancel = Some(cancel);
        self
    }

    pub fn with_tun_cfg(mut self, tun_cfg: tun::Configuration) -> Self {
        self.tun_cfg = Some(tun_cfg);
        self
    }

    pub fn with_auth<B>(self, auth: B) -> ServerBuilder<B> {
        ServerBuilder {
            auth_handler: auth,
            cancel: self.cancel,
            tun_cfg: self.tun_cfg,
        }
    }
}

impl<A: AuthStrategy + 'static> ServerBuilder<A> {
    pub async fn build(self) -> Result<Server> {
        Server::spawn(self).await
    }
}

impl Default for ServerBuilder {
    fn default() -> Self {
        Self::new()
    }
}

pub struct Server {
    cancel: CancellationToken,
    _cancel_guard: DropGuard,
    router_shutdown_rx: tokio::sync::oneshot::Receiver<anyhow::Result<()>>,
}

impl Server {
    pub fn builder() -> ServerBuilder {
        ServerBuilder::new()
    }

    async fn spawn<A: AuthStrategy + 'static>(builder: ServerBuilder<A>) -> Result<Self> {
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

        let tun_device = tun::create_as_async(&tun_cfg)
            .wrap_err("failed to create tun network device")
            .with_suggestion(|| "try running as root on linux")?;

        let endpoint = iroh::Endpoint::builder()
            // .discovery_n0()
            .bind()
            .await
            .map_err(|err| eyre!(format!("{err}")))
            .wrap_err("failed to bind iroh endpoint")?;

        let auth_handler = builder.auth_handler;
        let app_handler = Arc::new(crate::iroh_protocol::ApplicationProtocol::new(
            cancel.child_token(),
            tun_device,
        ));
        let handler = crate::auth::Auth(auth_handler, app_handler);
        let router = iroh::protocol::Router::builder(endpoint)
            .accept(handler.alpn(), handler)
            .spawn();
        let (router_shutdown_tx, router_shutdown_rx) = tokio::sync::oneshot::channel();
        let router_cancel = cancel.child_token();
        let router_clone = router.clone();
        tokio::task::spawn(async move {
            router_cancel.cancelled().await;
            let shutdown_result = router_clone.shutdown().await;
            let _ = router_shutdown_tx.send(shutdown_result);
        });

        Ok(Self {
            cancel,
            _cancel_guard: cancel_guard,
            router_shutdown_rx,
        })
    }

    /// Wait for all tasks to finish and propagate any errors.
    ///
    /// This is preferable to silently discarding erros in `Drop`.
    pub async fn join(self) -> Result<()> {
        self.router_shutdown_rx
            .await
            .wrap_err("router shutdown task panicked")?
            .map_err(|err| eyre!(format!("{err}")))
            .wrap_err("failed to shutdown router properly")
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
