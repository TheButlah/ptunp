use std::sync::{Arc, OnceLock};

use anyhow::Context as _;
use color_eyre::eyre::Result;
use futures::{FutureExt as _, TryFutureExt, future::BoxFuture};
use iroh::{endpoint::Connection, protocol::ProtocolHandler};

/// An auth strategy.
///
/// Accepting peers can support multiple auth strategies - each one becomes its own
/// ALPN and will be given to [`iroh::protocol::Router`].
pub trait AuthStrategy: Send + Sync + std::fmt::Debug + 'static {
    type Error: std::error::Error + Send + Sync + 'static;

    const ALPN_SUFFIX: &'static str;

    fn authenticate(
        &self,
        connection: Connection,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send + Sync + 'static;
}

#[derive(Debug)]
pub struct NoAuth;

impl AuthStrategy for NoAuth {
    type Error = std::convert::Infallible;

    const ALPN_SUFFIX: &str = "/noauth";

    fn authenticate(
        &self,
        _connection: Connection,
    ) -> impl Future<Output = Result<(), Self::Error>> + 'static {
        std::future::ready(Ok(()))
    }
}

#[derive(Debug)]
pub(crate) struct Auth<A: AuthStrategy, P: ProtocolHandler>(pub A, pub Arc<P>);

impl<A: AuthStrategy, P: ProtocolHandler> Auth<A, P> {
    // TODO: write my own const_concat
    pub fn alpn(&self) -> &'static str {
        static S: OnceLock<String> = OnceLock::new();
        S.get_or_init(|| format!("{}{}", crate::ALPN_PREFIX, A::ALPN_SUFFIX))
    }
}

impl<A: AuthStrategy, P: ProtocolHandler> ProtocolHandler for Auth<A, P> {
    fn accept(&self, connection: Connection) -> BoxFuture<'static, anyhow::Result<()>> {
        let auth_fut = self
            .0
            .authenticate(connection.clone())
            .map(move |result| result.context("error during authentication"));
        // Necessary because the future cannot hold on to `self` and we want to defer
        // creation of the inner handler's future till after authentication is completed
        let inner_handler = self.1.clone();
        Box::pin(auth_fut.and_then(move |()| inner_handler.accept(connection)))
    }
}
