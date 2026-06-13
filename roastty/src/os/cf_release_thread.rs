//! Dedicated CoreFoundation release worker.
//!
//! Faithful to the performance reason behind upstream `os/cf_release_thread.zig`:
//! temporary CoreFoundation/CoreText objects may run expensive release callbacks,
//! so hot shaping/rendering code can batch owned references and release them on a
//! small background thread instead of doing that work synchronously.

use std::ffi::c_void;
use std::ptr::NonNull;
use std::sync::mpsc::{sync_channel, SyncSender, TrySendError};
use std::sync::OnceLock;
use std::thread::{self, JoinHandle};

use objc2_core_foundation::{CFRetained, Type};

const MAILBOX_CAPACITY: usize = 64;

static GLOBAL_RELEASE_THREAD: OnceLock<Option<CfReleaseThread>> = OnceLock::new();

#[derive(Debug)]
struct CfReleaseRef(NonNull<c_void>);

// SAFETY: `CfReleaseRef` is an owned +1 CoreFoundation reference. The worker
// does not dereference the pointed-to value; it only transfers that ownership to
// `CFRelease`, which is the same operation upstream performs on its release
// thread for retained `CFTypeRef`s.
unsafe impl Send for CfReleaseRef {}

impl CfReleaseRef {
    fn from_retained<T: Type>(retained: CFRetained<T>) -> Self {
        Self(CFRetained::into_raw(retained).cast())
    }

    fn release(self) {
        release_raw(self.0);
    }
}

/// A pool of retained CF objects to release after their last use.
#[derive(Debug, Default)]
pub(crate) struct CfReleasePool {
    refs: Vec<CfReleaseRef>,
}

impl CfReleasePool {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn push<T: Type>(&mut self, retained: CFRetained<T>) {
        self.refs.push(CfReleaseRef::from_retained(retained));
    }

    pub(crate) fn flush(&mut self) {
        let Some(worker) = global_release_thread() else {
            self.release_now();
            return;
        };
        self.flush_with(worker);
    }

    fn flush_with(&mut self, worker: &CfReleaseThread) {
        if self.refs.is_empty() {
            return;
        }

        let refs = std::mem::take(&mut self.refs);
        if let Err(refs) = worker.release(refs) {
            release_all(refs);
        }
    }

    fn release_now(&mut self) {
        release_all(std::mem::take(&mut self.refs));
    }
}

impl Drop for CfReleasePool {
    fn drop(&mut self) {
        self.release_now();
    }
}

#[derive(Debug)]
struct CfReleaseThread {
    sender: Option<SyncSender<Vec<CfReleaseRef>>>,
    handle: Option<JoinHandle<()>>,
}

impl CfReleaseThread {
    fn spawn() -> std::io::Result<Self> {
        let (sender, receiver) = sync_channel::<Vec<CfReleaseRef>>(MAILBOX_CAPACITY);
        let handle = thread::Builder::new()
            .name("cf_release".to_string())
            .spawn(move || {
                let _ = crate::os::macos::set_thread_name(c"cf_release");
                while let Ok(refs) = receiver.recv() {
                    release_all(refs);
                }
            })?;

        Ok(Self {
            sender: Some(sender),
            handle: Some(handle),
        })
    }

    fn release(&self, refs: Vec<CfReleaseRef>) -> Result<(), Vec<CfReleaseRef>> {
        if refs.is_empty() {
            return Ok(());
        }

        let Some(sender) = &self.sender else {
            return Err(refs);
        };

        sender.try_send(refs).map_err(|err| match err {
            TrySendError::Full(refs) | TrySendError::Disconnected(refs) => refs,
        })
    }
}

impl Drop for CfReleaseThread {
    fn drop(&mut self) {
        self.sender.take();
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

fn global_release_thread() -> Option<&'static CfReleaseThread> {
    GLOBAL_RELEASE_THREAD
        .get_or_init(|| CfReleaseThread::spawn().ok())
        .as_ref()
}

fn release_all(refs: Vec<CfReleaseRef>) {
    for ref_ in refs {
        ref_.release();
    }
}

fn release_raw(ptr: NonNull<c_void>) {
    #[cfg(test)]
    release_test_hook(ptr);
    unsafe { CFRelease(ptr.as_ptr()) };
}

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFRelease(cf: *mut c_void);
}

#[cfg(test)]
fn release_test_hook(ptr: NonNull<c_void>) {
    let mut state = RELEASE_OBSERVER.lock().expect("release observer lock");
    let ptr = ptr.as_ptr() as usize;
    if state.watched.contains(&ptr) {
        state.releases.push((ptr, thread::current().id()));
    }
}

#[cfg(test)]
#[derive(Default)]
struct ReleaseObserver {
    watched: Vec<usize>,
    releases: Vec<(usize, thread::ThreadId)>,
}

#[cfg(test)]
static RELEASE_OBSERVER: std::sync::Mutex<ReleaseObserver> =
    std::sync::Mutex::new(ReleaseObserver {
        watched: Vec::new(),
        releases: Vec::new(),
    });

#[cfg(test)]
mod tests {
    use super::*;
    use objc2_core_foundation::CFString;
    use std::time::{Duration, Instant};

    struct ObservedRelease {
        ptr: usize,
    }

    impl Drop for ObservedRelease {
        fn drop(&mut self) {
            let mut state = RELEASE_OBSERVER.lock().expect("release observer lock");
            state.watched.retain(|&ptr| ptr != self.ptr);
            state.releases.retain(|&(ptr, _)| ptr != self.ptr);
        }
    }

    fn observed_string(text: &str) -> (CFRetained<CFString>, ObservedRelease) {
        let retained = CFString::from_str(text);
        let ptr = CFRetained::as_ptr(&retained).cast::<c_void>().as_ptr() as usize;
        {
            let mut state = RELEASE_OBSERVER.lock().expect("release observer lock");
            state.watched.push(ptr);
            state
                .releases
                .retain(|&(release_ptr, _)| release_ptr != ptr);
        }
        (retained, ObservedRelease { ptr })
    }

    fn release_threads(observed: &[&ObservedRelease]) -> Vec<thread::ThreadId> {
        let ptrs: Vec<usize> = observed.iter().map(|release| release.ptr).collect();
        RELEASE_OBSERVER
            .lock()
            .expect("release observer lock")
            .releases
            .iter()
            .filter_map(|&(ptr, thread)| ptrs.contains(&ptr).then_some(thread))
            .collect()
    }

    fn wait_for_release_count(
        observed: &[&ObservedRelease],
        count: usize,
    ) -> Vec<thread::ThreadId> {
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            let threads = release_threads(observed);
            if threads.len() >= count || Instant::now() >= deadline {
                return threads;
            }
            thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn pool_flush_releases_on_worker_thread() {
        let caller = thread::current().id();
        let worker = CfReleaseThread::spawn().expect("spawn release thread");
        let mut pool = CfReleasePool::new();
        let (first, first_release) = observed_string("worker-release-a");
        let (second, second_release) = observed_string("worker-release-b");

        pool.push(first);
        pool.push(second);
        pool.flush_with(&worker);

        let threads = wait_for_release_count(&[&first_release, &second_release], 2);
        assert_eq!(threads.len(), 2);
        assert!(
            threads.iter().all(|&id| id != caller),
            "release should run on the worker thread"
        );
    }

    #[test]
    fn worker_drop_drains_queued_refs() {
        let (first, first_release) = observed_string("drop-drain-a");
        let (second, second_release) = observed_string("drop-drain-b");
        {
            let worker = CfReleaseThread::spawn().expect("spawn release thread");
            let mut pool = CfReleasePool::new();
            pool.push(first);
            pool.push(second);
            pool.flush_with(&worker);
        }

        let threads = wait_for_release_count(&[&first_release, &second_release], 2);
        assert_eq!(threads.len(), 2);
    }

    #[test]
    fn pool_falls_back_to_synchronous_release_when_worker_is_closed() {
        let caller = thread::current().id();
        let mut pool = CfReleasePool::new();
        let (retained, release) = observed_string("fallback-release");
        pool.push(retained);
        pool.flush_with(&CfReleaseThread {
            sender: None,
            handle: None,
        });

        assert_eq!(release_threads(&[&release]), vec![caller]);
    }

    #[test]
    fn empty_pool_is_noop() {
        let (sentinel, release) = observed_string("empty-pool-sentinel");
        let mut pool = CfReleasePool::new();
        pool.flush();
        assert!(release_threads(&[&release]).is_empty());
        drop(release);
        drop(sentinel);
    }
}
