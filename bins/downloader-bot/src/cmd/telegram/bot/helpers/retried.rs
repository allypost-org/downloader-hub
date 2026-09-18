use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, OnceLock, Weak},
    time::Duration,
};

use teloxide::{RequestError, types::ChatId};
use tokio::{
    sync::Mutex,
    time::{Instant, sleep_until},
};
use tracing::{debug, trace};

const GLOBAL_PER_SECOND: usize = 30;
const CHAT_PER_SECOND: usize = 1;
const CHAT_PER_MINUTE: usize = 20;
const RETRY_SAFETY_MARGIN: Duration = Duration::from_secs(1);

#[derive(Default)]
struct ChatRateState {
    recent: VecDeque<Instant>,
    blocked_until: Option<Instant>,
}

#[derive(Default)]
struct RateState {
    global_recent: VecDeque<Instant>,
    chats: HashMap<ChatId, ChatRateState>,
}

/// Lock ordering (must be respected to avoid deadlock): `lanes` -> lane mutex -> `state`.
#[derive(Default)]
struct OutboundLimiter {
    state: Mutex<RateState>,
    lanes: Mutex<HashMap<ChatId, Weak<Mutex<()>>>>,
}

impl OutboundLimiter {
    async fn lane(&self, chat_id: ChatId) -> Arc<Mutex<()>> {
        let mut lanes = self.lanes.lock().await;
        lanes.retain(|_, lane| lane.strong_count() > 0);

        if let Some(lane) = lanes.get(&chat_id).and_then(Weak::upgrade) {
            return lane;
        }

        let lane = Arc::new(Mutex::new(()));
        lanes.insert(chat_id, Arc::downgrade(&lane));
        lane
    }

    async fn acquire(&self, chat_id: ChatId) {
        loop {
            let wait_until = {
                let now = Instant::now();
                let second_ago = now - Duration::from_secs(1);
                let minute_ago = now - Duration::from_mins(1);
                let mut state = self.state.lock().await;
                let RateState {
                    global_recent,
                    chats,
                } = &mut *state;

                while global_recent
                    .front()
                    .is_some_and(|sent| *sent <= second_ago)
                {
                    global_recent.pop_front();
                }
                chats.retain(|_, chat| {
                    while chat.recent.front().is_some_and(|sent| *sent <= minute_ago) {
                        chat.recent.pop_front();
                    }
                    !chat.recent.is_empty() || chat.blocked_until.is_some_and(|until| until > now)
                });

                let global_wait = (global_recent.len() >= GLOBAL_PER_SECOND)
                    .then(|| global_recent.front().copied())
                    .flatten()
                    .map(|sent| sent + Duration::from_secs(1));

                let chat = chats.entry(chat_id).or_default();
                while chat.recent.front().is_some_and(|sent| *sent <= minute_ago) {
                    chat.recent.pop_front();
                }

                let sent_last_second = chat
                    .recent
                    .iter()
                    .rev()
                    .take_while(|sent| **sent > second_ago)
                    .count();
                let second_wait = (sent_last_second >= CHAT_PER_SECOND)
                    .then(|| chat.recent.back().copied())
                    .flatten()
                    .map(|sent| sent + Duration::from_secs(1));
                let minute_wait = (chat.recent.len() >= CHAT_PER_MINUTE)
                    .then(|| chat.recent.front().copied())
                    .flatten()
                    .map(|sent| sent + Duration::from_mins(1));

                let wait_until = [global_wait, second_wait, minute_wait, chat.blocked_until]
                    .into_iter()
                    .flatten()
                    .max();

                let wait_until = if wait_until.is_some_and(|until| until > now) {
                    wait_until
                } else {
                    global_recent.push_back(now);
                    chat.recent.push_back(now);
                    None
                };
                drop(state);
                wait_until
            };

            let Some(wait_until) = wait_until else {
                return;
            };
            trace!(
                ?chat_id,
                ?wait_until,
                "Waiting for Telegram outbound permit"
            );
            sleep_until(wait_until).await;
        }
    }

    async fn defer_chat(&self, chat_id: ChatId, duration: Duration) {
        let until = Instant::now() + duration + RETRY_SAFETY_MARGIN;
        let mut state = self.state.lock().await;
        let blocked_until = &mut state.chats.entry(chat_id).or_default().blocked_until;
        if blocked_until.is_none_or(|current| current < until) {
            *blocked_until = Some(until);
        }
        drop(state);
    }
}

fn limiter() -> &'static OutboundLimiter {
    static LIMITER: OnceLock<OutboundLimiter> = OnceLock::new();
    LIMITER.get_or_init(OutboundLimiter::default)
}

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
        let lane = limiter().lane(chat_id).await;
        let _lane_guard = lane.lock().await;

        loop {
            limiter().acquire(chat_id).await;
            match generator(chat_id, payload.clone()).await {
                Ok(value) => return Ok(value),
                Err(RequestError::RetryAfter(seconds)) => {
                    let retry_after = seconds.duration();
                    limiter().defer_chat(chat_id, retry_after).await;
                    debug!(?retry_after, "Telegram rate-limited the chat");
                }
                Err(RequestError::MigrateToChatId(new_chat_id)) => {
                    debug!(?new_chat_id, "Telegram requested we migrate to a new chat");
                    chat_id = new_chat_id;
                    break;
                }
                Err(error) => return Err(error),
            }
        }
    }
}
