use app_peer_comms::PeeringEndpoint;
use futures::StreamExt;
use tracing::{debug, info, trace};

pub async fn run() -> super::ComponentResult {
    let pe = PeeringEndpoint::global();

    {
        let node_id = pe.endpoint_id().await;
        trace!(id = ?node_id, "Initialized peering endpoint");
    }

    let topic = pe.gossip_subscribe().await?;

    trace!(topic = ?pe.topic_id, "Initialized gossip topic subscriber");

    info!("Component ready");

    let (_sender, mut recv) = topic.split();
    while let Some(event) = recv.next().await {
        debug!(?event, "Inbound gossip event (ignored)");
    }

    Ok(())
}
