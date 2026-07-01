use std::sync::Arc;

use tracing::error;

use crate::cmd::central::{
    broadcaster::{BroadcastAudience, Broadcaster},
    components::worker_api::{auth::ValidAuth, event_handler::socket_sender::SocketSender},
};

pub async fn handle_broadcasts(sender: Arc<SocketSender>, auth: ValidAuth) {
    // println!("\n\n=============");
    // dbg!("Handling broadcasts", &auth);
    // println!("=============\n");
    let mut receiver = Broadcaster::recv_from_now();
    while let Ok(msg) = receiver.recv().await {
        // println!("\n\n=============");
        // dbg!("Received broadcast", &msg);
        // println!("=============\n\n");
        let Some(msg) = msg.get_if_for_me(&[
            BroadcastAudience::Socket(sender.id()),
            BroadcastAudience::Authed(auth.authed_id.clone()),
        ]) else {
            continue;
        };

        if let Err(e) = sender.send_message_boxed(msg).await {
            error!(?e, "Failed to forward broadcast message to clients");
        }
    }
}
