use std::{
    convert::Into,
    net::{SocketAddrV4, SocketAddrV6},
    path::Path,
    sync::{Arc, OnceLock},
    time::Duration,
};

use anyhow::Context;
use app_config::common::{BlobConfig, PeerCommsCommonConfig};
pub use app_database::entity::accounts::{AccountPlaceRef, AccountUserRef};
pub use app_requests::install_default_crypto_provider;
use futures::StreamExt;
use iroh::{
    Endpoint, EndpointAddr, RelayMode, RelayUrl, SecretKey, Watcher,
    address_lookup::{DnsAddressLookup, MemoryLookup},
    endpoint::Connection,
    protocol::{Router, RouterBuilder},
};
pub use iroh::{
    EndpointAddr as IrohEndpointAddr, EndpointId,
    endpoint::Connection as IrohConnection,
    protocol::{AcceptError as IrohAcceptError, ProtocolHandler as IrohProtocolHandler},
};
pub use iroh_blobs::{
    BlobFormat as IrohBlobFormat, Hash as IrohHash, HashAndFormat as IrohHashAndFormat,
    api::{
        blobs::{AddPathOptions as IrohAddPathOptions, ImportMode as IrohImportMode},
        proto::BlobStatus as IrohBlobStatus,
    },
    get::request::GetBlobItem as IrohGetBlobItem,
    ticket::BlobTicket as IrohBlobTicket,
};
use iroh_blobs::{
    BlobsProtocol,
    api::{
        blobs::AddBytesOptions,
        downloader::{DownloadProgress, Downloader},
    },
    get::{Stats as IrohBlobStats, request::GetBlobResult},
    provider::events::{
        ConnectMode, EventMask, EventSender, ObserveMode, ProviderMessage, RequestMode,
        RequestUpdate, ThrottleMode,
    },
    store::fs as blobs_store_fs,
};
use iroh_gossip::{
    api::{ApiError, GossipTopic},
    net::Gossip,
};
pub use iroh_gossip::{
    api::{Event as GossipEvent, GossipSender as IrohGossipSender, GossipTopic as IrohGossipTopic},
    proto::TopicId,
};
use iroh_mdns_address_lookup::MdnsAddressLookup;
pub use irpc;
pub use irpc_iroh;
use tokio::{
    fs::File,
    io::AsyncWriteExt,
    sync::{RwLock, mpsc},
};
use tracing::{debug, error, info, trace, warn};
use url::Url;

pub mod helpers;
pub mod message;
pub mod rpc;
pub mod ticket;

pub struct PeeringEndpointBuilder {
    config: PeerCommsCommonConfig,
    topic_id: TopicId,
    peers: Vec<EndpointAddr>,
    refresh_url: Option<Url>,
    refresh_token: Option<Arc<str>>,
    main_node_id: Option<EndpointId>,
    router_hook: Option<Box<dyn FnOnce(RouterBuilder) -> RouterBuilder + Send + 'static>>,
}
impl PeeringEndpointBuilder {
    const fn new(config: PeerCommsCommonConfig, topic_id: TopicId) -> Self {
        Self {
            config,
            topic_id,
            peers: vec![],
            refresh_url: None,
            main_node_id: None,
            refresh_token: None,
            router_hook: None,
        }
    }

    #[must_use]
    pub fn with_peers(mut self, peers: Vec<EndpointAddr>) -> Self {
        self.peers = peers;
        self
    }

    #[must_use]
    pub fn with_router_hook<F>(mut self, hook: F) -> Self
    where
        F: FnOnce(RouterBuilder) -> RouterBuilder + Send + 'static,
    {
        self.router_hook = Some(Box::new(hook));
        self
    }

    #[must_use]
    pub fn with_refresh_url(mut self, refresh_url: Option<Url>) -> Self {
        self.refresh_url = refresh_url;
        self
    }

    #[must_use]
    pub fn with_refresh_token(mut self, refresh_token: Option<Arc<str>>) -> Self {
        self.refresh_token = refresh_token;
        self
    }

    #[must_use]
    pub const fn with_main_node(mut self, id: Option<EndpointId>) -> Self {
        self.main_node_id = id;
        self
    }

    pub async fn build(self) -> Result<PeeringEndpoint, Box<dyn std::error::Error + Send + Sync>> {
        PeeringEndpoint::create(self).await
    }
}

#[derive(Debug, Clone)]
pub struct PeeringEndpoint {
    pub router: Router,
    pub topic_id: TopicId,
    pub gossip: Gossip,
    pub blobs: BlobsProtocol,
    pub peers: Arc<RwLock<Vec<EndpointAddr>>>,
    pub refresh_url: Option<Url>,
    pub main_node_id: Option<EndpointId>,
    node_addr: Arc<RwLock<EndpointAddr>>,
    join_ticket: Arc<RwLock<ticket::Ticket>>,
    downloader: OnceLock<Downloader>,
}
impl PeeringEndpoint {
    #[must_use]
    pub const fn builder(
        config: PeerCommsCommonConfig,
        topic_id: TopicId,
    ) -> PeeringEndpointBuilder {
        PeeringEndpointBuilder::new(config, topic_id)
    }
}
impl PeeringEndpoint {
    #[must_use]
    pub const fn trace_span_name() -> &'static str {
        "peering"
    }
}

impl PeeringEndpoint {
    #[must_use]
    pub fn with_endpoint_addr_watcher(self) -> Self {
        let node_addr = self.node_addr.clone();
        let join_ticket = self.join_ticket.clone();
        let peers = self.peers.clone();

        tokio::spawn({
            let mut update_stream = self.router.endpoint().watch_addr().stream_updates_only();

            async move {
                while let Some(new_me) = update_stream.next().await {
                    debug!(target: PeeringEndpoint::trace_span_name(), ?new_me, "Got new node address");
                    *node_addr.write().await = new_me.clone();
                    let old_ticket = join_ticket.read().await.clone();
                    *join_ticket.write().await = ticket::Ticket {
                        topic: self.topic_id,
                        peers: Arc::from(
                            peers
                                .read()
                                .await
                                .iter()
                                .chain([&new_me])
                                .cloned()
                                .collect::<Vec<_>>(),
                        ),
                        main: new_me,
                        refresh_url: old_ticket.refresh_url,
                        refresh_token: old_ticket.refresh_token,
                    };
                }
            }
        });

        self
    }

    pub async fn endpoint_addr(&self) -> EndpointAddr {
        self.node_addr.read().await.clone()
    }

    pub async fn endpoint_id(&self) -> EndpointId {
        self.endpoint_addr().await.id
    }

    pub async fn join_ticket(&self) -> ticket::Ticket {
        self.join_ticket.read().await.clone()
    }

    #[must_use]
    pub fn secret_key(&self) -> &SecretKey {
        self.router.endpoint().secret_key()
    }

    pub async fn gossip_subscribe(&self) -> Result<GossipTopic, ApiError> {
        let peers = self.peers.read().await.iter().map(|x| x.id).collect();

        trace!(target: PeeringEndpoint::trace_span_name(), topic = ?self.topic_id, ?peers, "Initializing gossip subscribe");

        self.gossip.subscribe(self.topic_id, peers).await
    }
}

impl PeeringEndpoint {
    #[must_use]
    pub fn downloader(&self) -> &Downloader {
        self.downloader
            .get_or_init(|| self.blobs.store().downloader(self.router.endpoint()))
    }

    #[must_use]
    pub fn download_with(
        &self,
        downloader: &Downloader,
        ticket: &IrohBlobTicket,
    ) -> DownloadProgress {
        let addrs = iroh_blobs::api::downloader::Shuffled::new(vec![ticket.addr().id]);
        downloader.download(ticket.hash_and_format(), addrs)
    }

    #[must_use]
    pub fn download(&self, ticket: &IrohBlobTicket) -> DownloadProgress {
        self.download_with(self.downloader(), ticket)
    }
}

impl PeeringEndpoint {
    pub async fn directly_connect_to(&self, addr: EndpointAddr) -> anyhow::Result<Connection> {
        static CONN_CACHE: std::sync::LazyLock<moka::future::Cache<EndpointAddr, Connection>> =
            std::sync::LazyLock::new(|| {
                moka::future::Cache::builder()
                    .time_to_idle(Duration::from_mins(2))
                    .max_capacity(30)
                    .build()
            });

        let entry = CONN_CACHE.entry(addr.clone());

        let res = entry
            .and_try_compute_with(|maybe_conn| async move {
                let Some(conn) = maybe_conn else {
                    trace!(target: PeeringEndpoint::trace_span_name(), ?addr, "No connection to peer, creating");
                    match self.directly_connect_to_impl(addr).await {
                        Ok(conn) => {
                            return Ok(moka::ops::compute::Op::Put(conn));
                        }
                        Err(e) => {
                            return Err(e);
                        }
                    }
                };

                if conn.value().close_reason().is_some() {
                    trace!(target: PeeringEndpoint::trace_span_name(), ?addr, "Connection to peer closed, re-connecting");
                    match self.directly_connect_to_impl(addr.clone()).await {
                        Ok(conn) => {
                            return Ok(moka::ops::compute::Op::Put(conn));
                        }
                        Err(e) => {
                            return Err(e);
                        }
                    }
                }

                trace!(target: PeeringEndpoint::trace_span_name(), ?addr, "Connection to peer is still open, reusing");

                Ok(moka::ops::compute::Op::Nop)
            })
            .await
            .context("Failed to connect to peer")?
            .into_entry()
            .map(moka::Entry::into_value);

        let Some(conn) = res else {
            anyhow::bail!("Failed to connect to peer");
        };

        Ok(conn)
    }

    async fn directly_connect_to_impl(&self, addr: EndpointAddr) -> anyhow::Result<Connection> {
        debug!(target: PeeringEndpoint::trace_span_name(), ?addr, "Directly connecting to peer");
        self.router
            .endpoint()
            .connect(addr, iroh_blobs::ALPN)
            .await
            .map_err(|e| anyhow::anyhow!(e).context("Failed to directly connect to peer"))
    }

    #[must_use]
    pub fn blob_downloader(&self, conn: Connection, hash: iroh_blobs::Hash) -> GetBlobResult {
        trace!(target: PeeringEndpoint::trace_span_name(), ?hash, "Initializing blob download");
        iroh_blobs::get::request::get_blob(conn, hash)
    }

    pub async fn download_blob_to(
        &self,
        get: &mut GetBlobResult,
        file: &mut File,
    ) -> anyhow::Result<IrohBlobStats> {
        debug!(target: PeeringEndpoint::trace_span_name(), ?file, "Downloading blob");
        let mut received = 0u64;
        let stats = loop {
            match get.next().await {
                Some(IrohGetBlobItem::Item(item)) => match item {
                    bao_tree::io::BaoContentItem::Leaf(leaf) => {
                        received += leaf.data.len() as u64;
                        tokio::io::AsyncWriteExt::write_all(file, &leaf.data)
                            .await
                            .context("Could not write to file")?;
                    }
                    bao_tree::io::BaoContentItem::Parent(_parent) => {}
                },
                Some(IrohGetBlobItem::Done(stats)) => {
                    break stats;
                }
                Some(IrohGetBlobItem::Error(err)) => {
                    anyhow::bail!("Error while streaming blob (received {received} bytes): {err}");
                }
                None => {
                    anyhow::bail!("Stream ended unexpectedly (received {received} bytes)");
                }
            }
        };

        trace!(target: PeeringEndpoint::trace_span_name(), ?stats, "Blob downloaded");

        Ok(stats)
    }

    #[tracing::instrument(skip_all, fields(hash = %ticket.hash()))]
    pub async fn download_ticket(
        &self,
        ticket: IrohBlobTicket,
        to: &Path,
    ) -> Result<(File, IrohBlobStats), Box<dyn std::error::Error + Send + Sync>> {
        let mut file = File::create(to).await?;

        let stats = self.download_ticket_into(ticket, &mut file).await?;

        Ok((file, stats))
    }

    pub async fn download_ticket_into(
        &self,
        ticket: IrohBlobTicket,
        file: &mut File,
    ) -> Result<IrohBlobStats, Box<dyn std::error::Error + Send + Sync>> {
        debug!(target: PeeringEndpoint::trace_span_name(), ?ticket, ?file, "Downloading blob");

        let addr = ticket.addr().clone();
        let conn = self.directly_connect_to(addr).await?;

        trace!(target: PeeringEndpoint::trace_span_name(), "Connected to peer");

        let mut progress = self.blob_downloader(conn, ticket.hash());

        trace!(target: PeeringEndpoint::trace_span_name(), "Downloading blob");

        let stats = self.download_blob_to(&mut progress, file).await?;

        file.flush().await?;

        debug!(target: PeeringEndpoint::trace_span_name(), ?stats, "Downloaded blob");

        Ok(stats)
    }
}

impl PeeringEndpoint {
    #[must_use]
    pub const fn expiring_tag_prefix() -> &'static str {
        "__expiring-"
    }

    #[must_use]
    pub fn expiring_tag_name(expires: &jiff::Timestamp) -> String {
        let formatted = expires.strftime("%Y-%m-%dT%H:%M:%SZ").to_string();
        format!("{}{formatted}", Self::expiring_tag_prefix())
    }

    pub async fn delete_expired_tags(&self) -> Result<u64, iroh_blobs::api::RequestError> {
        let from = iroh_blobs::api::Tag::from(Self::expiring_tag_prefix());
        let to = iroh_blobs::api::Tag::from(Self::expiring_tag_name(&jiff::Timestamp::now()));

        trace!(target: PeeringEndpoint::trace_span_name(), ?from, ?to, "Deleting expired tags");

        self.blobs.tags().delete_range(from..to).await
    }

    pub async fn create_expiring_tag(
        &self,
        hashes: &[iroh_blobs::Hash],
        expires: jiff::Timestamp,
    ) -> Result<Option<String>, iroh_blobs::api::RequestError> {
        if hashes.is_empty() {
            return Ok(None);
        }
        let tag_name = Self::expiring_tag_name(&expires);

        if hashes.len() == 1 {
            let hash = hashes[0];
            self.blobs.store().tags().set(&tag_name, hash).await?;
            return Ok(Some(tag_name));
        }

        let hs = hashes
            .iter()
            .copied()
            .collect::<iroh_blobs::hashseq::HashSeq>();

        self.blobs
            .store()
            .add_bytes_with_opts(AddBytesOptions {
                data: hs.into(),
                format: iroh_blobs::BlobFormat::HashSeq,
            })
            .with_named_tag(&tag_name)
            .await?;

        Ok(Some(tag_name))
    }
}

impl PeeringEndpoint {
    async fn create(
        builder: PeeringEndpointBuilder,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let secret_key = builder
            .config
            .secret_key
            .map_or_else(SecretKey::generate, |key| SecretKey::from_bytes(&key));

        let endpoint = Self::create_endpoint(
            builder.config.clone(),
            builder.peers.clone(),
            secret_key.clone(),
        )
        .await?;
        let me = endpoint.addr();

        let join_ticket = ticket::Ticket {
            topic: builder.topic_id,
            main: me.clone(),
            peers: Arc::from(builder.peers.clone()),
            refresh_url: builder.refresh_url.clone(),
            refresh_token: builder.refresh_token,
        };

        let (router, gossip, blobs) =
            Self::create_router(endpoint, builder.config.blob, builder.router_hook).await?;

        Ok(Self {
            router,
            gossip,
            blobs,
            topic_id: builder.topic_id,
            refresh_url: builder.refresh_url,
            node_addr: Arc::new(RwLock::new(me)),
            peers: Arc::new(RwLock::new(builder.peers)),
            join_ticket: Arc::new(RwLock::new(join_ticket)),
            main_node_id: builder.main_node_id,
            downloader: OnceLock::new(),
        }
        .with_endpoint_addr_watcher())
    }

    async fn create_endpoint(
        config: PeerCommsCommonConfig,
        peers: Vec<EndpointAddr>,
        secret_key: SecretKey,
    ) -> Result<Endpoint, Box<dyn std::error::Error + Send + Sync>> {
        trace!(target: PeeringEndpoint::trace_span_name(), ?config, "Creating endpoint from config");

        debug!(
            target: PeeringEndpoint::trace_span_name(),
            secret_key = data_encoding::HEXLOWER.encode(&secret_key.to_bytes()),
            "Got secret key"
        );

        let relay_mode = match (config.relay.no_relay, config.relay.relays) {
            (false, relays) if relays.is_empty() => RelayMode::Default,
            (false, relays) => RelayMode::Custom(
                relays
                    .into_iter()
                    .map(|url| {
                        let n: RelayUrl = url.into();
                        n
                    })
                    .collect(),
            ),
            (true, relays) if relays.is_empty() => RelayMode::Disabled,
            (true, _) => {
                return Err("Cannot have relays and no_relay set".into());
            }
        };

        trace!(target: PeeringEndpoint::trace_span_name(), ?relay_mode, "Initialized relay mode");

        let static_provider = MemoryLookup::new();
        if !peers.is_empty() {
            debug!(target: PeeringEndpoint::trace_span_name(), ?peers, "Adding peers to static provider");
            for peer in peers {
                static_provider.add_endpoint_info(peer);
            }
        }

        let socket_addr_v4 = SocketAddrV4::new(
            config.bind.addr_v4,
            config.bind.port_v4.unwrap_or(config.bind.port),
        );
        trace!(target: PeeringEndpoint::trace_span_name(), ?socket_addr_v4, "Initialized IPv4 socket addr");

        let socket_addr_v6 = SocketAddrV6::new(
            config.bind.addr_v6,
            config.bind.port_v6.unwrap_or(config.bind.port),
            0,
            0,
        );
        trace!(target: PeeringEndpoint::trace_span_name(), ?socket_addr_v6, "Initialized IPv6 socket addr");

        let endpoint = Endpoint::builder(iroh::endpoint::presets::N0)
            .secret_key(secret_key)
            .address_lookup(DnsAddressLookup::n0_dns())
            .address_lookup(static_provider.clone())
            .relay_mode(relay_mode.clone())
            .bind_addr(socket_addr_v4)?
            .bind_addr(socket_addr_v6)?
            .bind()
            .await?;

        endpoint
            .address_lookup()?
            .add(MdnsAddressLookup::builder().build(endpoint.id())?);

        debug!(target: PeeringEndpoint::trace_span_name(), on = ?endpoint.bound_sockets(), id = %endpoint.id().fmt_short(), "Initialized endpoint");

        if !matches!(relay_mode, RelayMode::Disabled) {
            trace!(target: PeeringEndpoint::trace_span_name(), "Waiting for endpoint to come online");
            let timeout = Duration::from_secs(iroh::NET_REPORT_TIMEOUT * 2);
            let res = tokio::time::timeout(timeout, endpoint.online()).await;
            if res.is_err() {
                error!(target: PeeringEndpoint::trace_span_name(), ?timeout, "Endpoint failed to come online");
                return Err("Endpoint failed to come online".into());
            }
            debug!(target: PeeringEndpoint::trace_span_name(), "Endpoint is online now");
        }

        info!(target: PeeringEndpoint::trace_span_name(), "Peering endpoint is ready");

        Ok(endpoint)
    }

    async fn create_router(
        endpoint: Endpoint,
        blob_config: BlobConfig,
        router_hook: Option<Box<dyn FnOnce(RouterBuilder) -> RouterBuilder + Send + 'static>>,
    ) -> Result<(Router, Gossip, BlobsProtocol), Box<dyn std::error::Error + Send + Sync>> {
        trace!(target: PeeringEndpoint::trace_span_name(), ?blob_config, endpoint = ?endpoint.id(), "Creating router");
        let gossip = Gossip::builder().spawn(endpoint.clone());
        let blobs = match blob_config.store {
            Some(path) => {
                debug!(target: PeeringEndpoint::trace_span_name(), ?path, "Using filesystem blob store");
                let store_opts = blobs_store_fs::options::Options {
                    gc: Some(iroh_blobs::store::GcConfig {
                        add_protected: None,
                        interval: Duration::from_secs(10),
                    }),
                    ..blobs_store_fs::options::Options::new(&path)
                };
                let store =
                    blobs_store_fs::FsStore::load_with_opts(path.join("blobs.db"), store_opts)
                        .await?;
                trace!(target: PeeringEndpoint::trace_span_name(), ?store, "Filesystem blob store loaded");
                let (events, event_rx) = EventSender::channel(
                    256,
                    EventMask {
                        connected: ConnectMode::None,
                        get: RequestMode::NotifyLog,
                        get_many: RequestMode::NotifyLog,
                        push: RequestMode::None,
                        observe: ObserveMode::None,
                        throttle: ThrottleMode::None,
                    },
                );
                tokio::spawn(log_blob_provider_events(event_rx));
                BlobsProtocol::new(&store, Some(events))
            }
            None => {
                debug!(target: PeeringEndpoint::trace_span_name(), "Using in-memory blob store");
                let store = iroh_blobs::store::mem::MemStore::new();
                BlobsProtocol::new(&store, None)
            }
        };

        let builder = Router::builder(endpoint)
            .accept(iroh_gossip::ALPN, gossip.clone())
            .accept(iroh_blobs::ALPN, blobs.clone());
        let builder = match router_hook {
            Some(hook) => hook(builder),
            None => builder,
        };
        let router = builder.spawn();

        debug!(target: PeeringEndpoint::trace_span_name(), "Router is ready");

        Ok((router, gossip, blobs))
    }
}

async fn log_blob_provider_events(mut rx: mpsc::Receiver<ProviderMessage>) {
    while let Some(msg) = rx.recv().await {
        match msg {
            ProviderMessage::GetRequestReceivedNotify(msg) => {
                spawn_request_update_logger(
                    msg.inner.connection_id,
                    msg.inner.request_id,
                    format!("{:?}", msg.inner.request),
                    msg.rx,
                );
            }
            ProviderMessage::GetManyRequestReceivedNotify(msg) => {
                spawn_request_update_logger(
                    msg.inner.connection_id,
                    msg.inner.request_id,
                    format!("{:?}", msg.inner.request),
                    msg.rx,
                );
            }
            _ => {}
        }
    }
}

fn spawn_request_update_logger(
    connection_id: u64,
    request_id: u64,
    request: String,
    mut rx: irpc::channel::mpsc::Receiver<RequestUpdate>,
) {
    tokio::spawn(async move {
        trace!(
            target: PeeringEndpoint::trace_span_name(),
            connection_id, request_id, request, "blob request received"
        );
        while let Ok(Some(update)) = rx.recv().await {
            match update {
                RequestUpdate::Started(started) => {
                    debug!(
                        target: PeeringEndpoint::trace_span_name(),
                        connection_id, request_id, hash = %started.hash, size = started.size,
                        "blob transfer started"
                    );
                }
                RequestUpdate::Aborted(aborted) => {
                    warn!(
                        target: PeeringEndpoint::trace_span_name(),
                        connection_id, request_id, stats = ?aborted.stats,
                        "blob transfer aborted"
                    );
                }
                RequestUpdate::Completed(completed) => {
                    debug!(
                        target: PeeringEndpoint::trace_span_name(),
                        connection_id, request_id, stats = ?completed.stats,
                        "blob transfer completed"
                    );
                }
                RequestUpdate::Progress(_) => {}
            }
        }
    });
}
