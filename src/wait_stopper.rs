use std::sync::{Condvar, Mutex};
use std::thread::sleep;
use std::time::Duration;


#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(transparent)]
pub(crate) struct StopResult(bool);
impl StopResult {
    #[inline] pub fn wants_to_stop(&self) -> bool { self.0 }

    #[inline] pub fn new_wants_to_stop() -> Self { Self(true) }
    #[inline] pub fn new_does_not_want_to_stop() -> Self { Self(false) }
}


#[derive(Debug)]
pub(crate) struct WaitStopper {
    mutex: Mutex<StopResult>,
    cond_var: Condvar,
}
impl WaitStopper {
    pub fn new() -> Self {
        let mutex = Mutex::new(StopResult::new_does_not_want_to_stop());
        let cond_var = Condvar::new();
        Self {
            mutex,
            cond_var,
        }
    }

    pub fn wait_until_stop_timeout(&self, timeout: Duration) -> StopResult {
        let guard = self.mutex.lock()
            .expect("mutex is poisoned");
        if guard.wants_to_stop() {
            return *guard;
        }
        let (guard, _timeout_result) = self.cond_var.wait_timeout(guard, timeout)
            .expect("mutex is poisoned");
        return *guard;
    }

    pub fn stop(&self) {
        {
            let mut guard = self.mutex.lock()
                .expect("mutex is poisoned");
            *guard = StopResult::new_wants_to_stop();
        }
        self.cond_var.notify_all();
    }

    pub fn wait_until_stop_timeout_opt(stopper: Option<&WaitStopper>, timeout: Duration) -> StopResult {
        if let Some(s) = stopper {
            s.wait_until_stop_timeout(timeout)
        } else {
            sleep(timeout);
            StopResult::new_does_not_want_to_stop()
        }
    }
}
