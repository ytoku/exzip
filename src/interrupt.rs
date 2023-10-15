use std::sync::atomic::{AtomicBool, Ordering};

static INTERRUPTED: AtomicBool = AtomicBool::new(false);

pub fn register_ctrlc() {
    ctrlc::set_handler(move || {
        INTERRUPTED.store(true, Ordering::SeqCst);
    })
    .expect("Error setting Ctrl-C handler");
}

#[inline]
pub fn interrupted() -> bool {
    INTERRUPTED.load(Ordering::SeqCst)
}
