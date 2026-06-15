use std::sync::{Arc, OnceLock};

static GLOBAL_DATA: OnceLock<Arc<GlobalData>> = OnceLock::new();

pub struct GlobalData {
    pub jwt_secret: Arc<str>,
}
impl GlobalData {
    pub fn init(data: Self) {
        _ = GLOBAL_DATA.set(Arc::new(data));
    }

    pub fn get() -> Arc<Self> {
        GLOBAL_DATA
            .get()
            .expect("Global data not initialized")
            .clone()
    }

    pub fn jwt_secret() -> Arc<str> {
        Self::get().jwt_secret.clone()
    }
}
