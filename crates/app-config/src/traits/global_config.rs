use std::sync::OnceLock;

pub trait GlobalConfig: Sized + 'static {
    fn global_instance() -> &'static OnceLock<Self>;

    fn init_global(conf: Self) -> Result<(), String> {
        Self::global_instance()
            .set(conf)
            .map_err(|_| "Config already initialized".to_string())
    }

    fn try_init_global<F>(f: F) -> &'static Self
    where
        F: FnOnce() -> Self,
    {
        Self::global_instance().get_or_init(f)
    }

    #[must_use]
    #[inline]
    fn get_global() -> Option<&'static Self> {
        Self::global_instance().get()
    }

    #[must_use]
    #[inline]
    fn initialized_global() -> bool {
        Self::global_instance().get().is_some()
    }
}
