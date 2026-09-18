use futures::StreamExt;
use tracing::{debug, info, trace};

use super::state::SharedCentralState;

pub async fn run(state: SharedCentralState) -> super::ComponentResult {
    let pe = state.peering.get().expect("peering endpoint initialized");

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
