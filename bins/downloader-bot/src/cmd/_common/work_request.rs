use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use tokio::sync::{OwnedSemaphorePermit, Semaphore};

pub type WorkRequestLockMap = HashMap<Arc<str>, Arc<Semaphore>>;

pub struct WorkRequestGuard {
    map: Arc<Mutex<WorkRequestLockMap>>,
    request_id: Arc<str>,
    semaphore: Arc<Semaphore>,
    _permit: OwnedSemaphorePermit,
}

impl WorkRequestGuard {
    pub fn is_processing(map: &Arc<Mutex<WorkRequestLockMap>>, request_id: &Arc<str>) -> bool {
        let Ok(locks) = map.lock() else {
            return false;
        };
        locks.contains_key(request_id)
    }

    pub fn try_acquire(map: Arc<Mutex<WorkRequestLockMap>>, request_id: Arc<str>) -> Option<Self> {
        let semaphore = {
            let Ok(mut locks) = map.lock() else {
                return None;
            };
            locks.get(&request_id).cloned().unwrap_or_else(|| {
                let lock = Arc::new(Semaphore::new(1));
                locks.insert(request_id.clone(), lock.clone());
                lock
            })
        };

        semaphore
            .clone()
            .try_acquire_owned()
            .ok()
            .map(|permit| Self {
                map,
                request_id,
                semaphore,
                _permit: permit,
            })
    }
}

impl Drop for WorkRequestGuard {
    fn drop(&mut self) {
        let Ok(mut locks) = self.map.lock() else {
            return;
        };
        if locks
            .get(&self.request_id)
            .is_some_and(|s| Arc::ptr_eq(s, &self.semaphore))
        {
            locks.remove(&self.request_id);
        }
    }
}
