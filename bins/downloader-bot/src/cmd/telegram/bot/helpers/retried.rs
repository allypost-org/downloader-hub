use teloxide::{RequestError, types::ChatId};
use tracing::{debug, trace};

#[tracing::instrument(skip_all, fields(chat_id = ?chat_id))]
pub async fn try_send_to_retrying<F, U, P>(
    mut chat_id: ChatId,
    payload: P,
    generator: Box<dyn Fn(ChatId, P) -> F + Send + Sync>,
) -> Result<U, RequestError>
where
    F: std::future::Future<Output = Result<U, RequestError>>,
    P: Clone,
{
    loop {
        let fut = generator(chat_id, payload.clone()).await;
        let err = match fut {
            Ok(x) => return Ok(x),
            Err(e) => e,
        };
        match err {
            RequestError::RetryAfter(secs) => {
                let dur = secs.duration();
                debug!(
                    ?dur,
                    "Telegram requested we wait for a bit before retrying send"
                );
                tokio::time::sleep(dur).await;
                trace!("Done sleeping");
            }
            RequestError::MigrateToChatId(new_chat_id) => {
                debug!(?new_chat_id, "Telegram requested we migrate to a new chat");
                chat_id = new_chat_id;
            }
            e => {
                return Err(e);
            }
        }
        debug!("Retrying send");
    }
}
