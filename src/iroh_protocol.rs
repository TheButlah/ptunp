use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use anyhow::anyhow;
use futures::future::BoxFuture;
use iroh::{
    endpoint::{Connection, VarInt},
    protocol::ProtocolHandler,
};
use tokio_util::sync::CancellationToken;
use tracing::{Instrument, debug, info_span};
use tun::AsyncDevice;

#[derive(derive_more::Debug)]
pub(crate) struct ApplicationProtocol {
    cancel: CancellationToken,
    // invariant: only the singular peer_task ever accesses this
    #[debug(skip)]
    tun: Arc<AsyncDevice>,
    has_peer: AtomicBool,
}

impl ApplicationProtocol {
    pub fn new(cancel: CancellationToken, tun: AsyncDevice) -> Self {
        Self {
            cancel,
            tun: Arc::new(tun),
            has_peer: AtomicBool::new(false),
        }
    }
}

impl ApplicationProtocol {
    fn accept_sync(&self, connection: Connection) -> anyhow::Result<()> {
        match self
            .has_peer
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        {
            Ok(has_peer) => assert!(
                !has_peer,
                "if we swapped successfully, we must not have had a peer"
            ),
            Err(has_peer) => {
                assert!(
                    has_peer,
                    "if we did not swap successfully, we must have had a peer already"
                );
                return Err(anyhow!("already have a peer"));
            }
        }

        let fut = task(
            self.cancel.child_token(),
            self.tun.clone(),
            connection.clone(),
        );
        let remote_node_id = connection.remote_node_id()?;
        let alpn = connection.alpn();
        tokio::task::spawn(
            async move {
                if let Err(err) = fut.await {
                    connection.close(VarInt::from_u32(500), b"internal server error");
                    tracing::error!("error in application protocol handler: {err:?}");
                }
            }
            .instrument(info_span!("application handler", ?alpn, ?remote_node_id)),
        );

        Ok(())
    }
}

impl ProtocolHandler for ApplicationProtocol {
    fn accept(&self, connection: Connection) -> BoxFuture<'static, anyhow::Result<()>> {
        Box::pin(std::future::ready(self.accept_sync(connection)))
    }
}

async fn task(
    cancel: CancellationToken,
    _tun: Arc<AsyncDevice>,
    _connection: Connection,
) -> color_eyre::Result<()> {
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
