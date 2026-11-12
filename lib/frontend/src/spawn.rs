use fluxemu_platform::MaybeSend;
use tokio::{runtime::Handle, task::JoinHandle};

pub(crate) fn spawn<F>(runtime: &Handle, future: F) -> JoinHandle<F::Output>
where
    F: Future + MaybeSend + 'static,
    F::Output: MaybeSend + 'static,
{
    cfg_select! {
        target_family = "wasm" => {
            let _ = runtime;
            tokio::task::spawn_local(future)
        }
        _ => runtime.spawn(future),
    }
}
