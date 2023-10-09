use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::sync::OnceLock;

static INTERRUPTED: OnceLock<Arc<AtomicBool>> = OnceLock::new();

pub fn register_ctrlc() {
    let setter = Arc::clone(INTERRUPTED.get_or_init(|| Arc::new(AtomicBool::new(false))));
    ctrlc::set_handler(move || {
        setter.store(true, Ordering::SeqCst);
    })
    .expect("Error setting Ctrl-C handler");
}

pub fn interrupted() -> bool {
    INTERRUPTED.get().unwrap().load(Ordering::SeqCst)
}
