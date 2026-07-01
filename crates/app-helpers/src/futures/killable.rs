pub fn spawn_killable(fut: impl std::future::Future<Output = ()> + 'static + Send) -> KillSwitch {
    let (sender, receiver) = tokio::sync::oneshot::channel::<()>();
    let fn_fut = Box::pin(fut);
    let receiver = receiver;
    _ = tokio::task::spawn(async move {
        tokio::select! {
            () = fn_fut => {},
            x = receiver => {
                drop(x);
            },
        }
    });

    KillSwitch { sender }
}

pub struct KillSwitch {
    sender: tokio::sync::oneshot::Sender<()>,
}

impl KillSwitch {
    pub fn kill(self) {
        let _ = self.sender.send(());
    }
}
