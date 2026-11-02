//! Ordering the publications of one file: its buffer's while it is open,
//! its disk copy's while it is not.
//!
//! While a file is open its indexes describe the editor's buffer, otherwise
//! the file on disk.  Unordered, an update of either can land after a newer
//! one: a disk parse that found the file closed finishing after the editor
//! opened it, a buffer's publish still writing when the file is closed and
//! re-read from disk, or a buffer from before a close and reopen finishing
//! after the reopened one.
//!
//! Every publication registers here before its first write and stays
//! registered until its last, the refreshes that follow the main indexes
//! included.  Registering reads `open_files` under this registry's lock:
//!
//! - A buffer publishes only while its file is open, and only once every
//!   other publication of the file has finished, so the buffer that takes
//!   its turn last writes last.
//! - The disk copy publishes only while its file is closed.  A re-read of a
//!   file closed while its buffer was still publishing waits for that
//!   publish ([`PublicationOrder::begin_disk`] with `after_buffer`).
//!
//! `didOpen`, `didChange` and `didSave` record the buffer in `open_files`
//! before they publish it, and `didClose` removes it before the file is
//! re-read, so of two registrations the second sees what the first saw and
//! either waits for it or steps aside.
//!
//! Only a thread that holds no publication waits.  A publication can lazily
//! load other files while it writes (repopulating the resolved-class cache
//! does), and were it to wait then, two publications could each wait for
//! the other.  A publication made while its thread holds another therefore
//! goes ahead; when that skips a wait it is logged, since none is expected
//! to.

use std::cell::Cell;
use std::collections::HashMap;
use std::marker::PhantomData;
use std::sync::Arc;

use parking_lot::{Condvar, Mutex, MutexGuard, RwLock};

thread_local! {
    /// How many publications this thread holds.
    static HELD: Cell<usize> = const { Cell::new(0) };
}

/// The publications of one URI that are running.
#[derive(Default)]
struct InFlight {
    disk: usize,
    buffer: usize,
}

/// The publications in progress, by URI.
#[derive(Default)]
pub(crate) struct PublicationOrder {
    in_flight: Mutex<HashMap<String, InFlight>>,
    /// Notified whenever a publication finishes.
    finished: Condvar,
    /// Publications blocked waiting for their turn, so a test can tell
    /// that one has reached the wait.
    #[cfg(test)]
    waiting: std::sync::atomic::AtomicUsize,
}

/// A registered publication of one URI, finished when dropped.
///
/// Not `Send`: the count of publications its thread holds is per thread.
pub(crate) struct Publication<'a> {
    order: &'a PublicationOrder,
    uri: String,
    buffer: bool,
    _thread_bound: PhantomData<*const ()>,
}

type OpenFiles = RwLock<HashMap<String, Arc<String>>>;

impl PublicationOrder {
    /// Register a publication of `uri`'s buffer, once no other publication
    /// of `uri` is running, or `None` if `uri` is not open by then.
    ///
    /// With `buffer`, also `None` unless `uri` still holds that very buffer:
    /// an edit, or a close and reopen, can have replaced it while its
    /// publication waited to start.  Checked by identity rather than by
    /// text, so a buffer replaced by an identical one steps aside too; the
    /// replacement publishes the same text itself.
    ///
    /// The wait is unbounded: giving up would let the other publication
    /// land last, which is what it exists to prevent.  A wait that runs
    /// long is logged instead.
    pub(crate) fn begin_buffer<'a>(
        &'a self,
        uri: &str,
        buffer: Option<&Arc<String>>,
        open_files: &OpenFiles,
    ) -> Option<Publication<'a>> {
        let mut in_flight = self.wait_until(uri, |running| running.disk + running.buffer == 0);
        let current = open_files
            .read()
            .get(uri)
            .is_some_and(|open| buffer.is_none_or(|buffer| Arc::ptr_eq(buffer, open)));
        current.then(|| self.register(&mut in_flight, uri, true))
    }

    /// Register a publication of `uri`'s disk copy, or `None` if `uri` is
    /// open in the editor, whose buffer then owns the file's indexes.
    ///
    /// With `after_buffer`, first wait for a publication of the buffer
    /// still running, which the file was closed during.  Without, the
    /// caller must not hold anything a buffer publication could wait on: a
    /// lazy parse's claim on the file, which a buffer's write can reach for
    /// when it loads a class.
    pub(crate) fn begin_disk<'a>(
        &'a self,
        uri: &str,
        open_files: &OpenFiles,
        after_buffer: bool,
    ) -> Option<Publication<'a>> {
        let mut in_flight = if after_buffer {
            self.wait_until(uri, |running| running.buffer == 0)
        } else {
            self.in_flight.lock()
        };
        if open_files.read().contains_key(uri) {
            return None;
        }
        Some(self.register(&mut in_flight, uri, false))
    }

    /// Lock the registry once `done` holds for `uri`'s running
    /// publications, or at once on a thread that holds a publication.
    fn wait_until(
        &self,
        uri: &str,
        done: impl Fn(&InFlight) -> bool,
    ) -> MutexGuard<'_, HashMap<String, InFlight>> {
        const LOG_AFTER: std::time::Duration = std::time::Duration::from_secs(10);
        let pending =
            |in_flight: &HashMap<String, InFlight>| in_flight.get(uri).is_some_and(|r| !done(r));
        let mut in_flight = self.in_flight.lock();
        if HELD.with(Cell::get) > 0 {
            if pending(&in_flight) {
                tracing::warn!(
                    "PHPantom: publishing {uri} without waiting for its other publications, \
                     because this thread is in the middle of one"
                );
            }
            return in_flight;
        }
        while pending(&in_flight) {
            #[cfg(test)]
            self.waiting
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            let timed_out = self
                .finished
                .wait_for(&mut in_flight, LOG_AFTER)
                .timed_out();
            #[cfg(test)]
            self.waiting
                .fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
            if timed_out {
                tracing::warn!(
                    "PHPantom: still waiting after {LOG_AFTER:?} for another publication \
                     of {uri} to finish"
                );
            }
        }
        in_flight
    }

    fn register<'a>(
        &'a self,
        in_flight: &mut HashMap<String, InFlight>,
        uri: &str,
        buffer: bool,
    ) -> Publication<'a> {
        let running = in_flight.entry(uri.to_owned()).or_default();
        if buffer {
            running.buffer += 1;
        } else {
            running.disk += 1;
        }
        HELD.with(|held| held.set(held.get() + 1));
        Publication {
            order: self,
            uri: uri.to_owned(),
            buffer,
            _thread_bound: PhantomData,
        }
    }

    /// Block until a publication is waiting for its turn.
    ///
    /// Panics after 10 s, so a publication that never waits fails the test
    /// instead of hanging it.
    #[cfg(test)]
    pub(crate) fn block_until_waited_on(&self) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while self.waiting.load(std::sync::atomic::Ordering::SeqCst) == 0 {
            assert!(
                std::time::Instant::now() < deadline,
                "no publication waited for another"
            );
            std::thread::yield_now();
        }
    }
}

impl Publication<'_> {
    /// Whether this publishes the file's buffer rather than its disk copy.
    pub(crate) fn is_buffer(&self) -> bool {
        self.buffer
    }
}

impl Drop for Publication<'_> {
    fn drop(&mut self) {
        HELD.with(|held| held.set(held.get() - 1));
        let mut in_flight = self.order.in_flight.lock();
        if let Some(running) = in_flight.get_mut(&self.uri) {
            if self.buffer {
                running.buffer -= 1;
            } else {
                running.disk -= 1;
            }
            if running.disk + running.buffer == 0 {
                in_flight.remove(&self.uri);
            }
        }
        drop(in_flight);
        self.order.finished.notify_all();
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Arc;

    use parking_lot::RwLock;

    use super::PublicationOrder;

    const URI: &str = "file:///a.php";
    const BUFFER: &str = "<?php";

    fn open(files: &RwLock<HashMap<String, Arc<String>>>, text: &str) {
        files
            .write()
            .insert(URI.to_owned(), Arc::new(text.to_owned()));
    }

    #[test]
    fn an_open_file_is_not_published_from_disk() {
        let order = PublicationOrder::default();
        let files = RwLock::new(HashMap::new());
        open(&files, BUFFER);
        assert!(order.begin_disk(URI, &files, true).is_none());
    }

    #[test]
    fn a_buffer_waits_for_a_disk_publication_registered_before_the_open() {
        let order = PublicationOrder::default();
        let files = RwLock::new(HashMap::new());
        let publication = order.begin_disk(URI, &files, true).unwrap();
        std::thread::scope(|s| {
            let opener = s.spawn(|| {
                open(&files, BUFFER);
                order.begin_buffer(URI, None, &files).is_some()
            });
            order.block_until_waited_on();
            drop(publication);
            assert!(opener.join().unwrap());
        });
        assert!(order.in_flight.lock().is_empty());
    }

    /// A file closed while its buffer was publishing is re-read from disk
    /// only once that publish is done.
    #[test]
    fn a_reread_after_a_close_waits_for_the_buffer_publishing_at_the_close() {
        let order = PublicationOrder::default();
        let files = RwLock::new(HashMap::new());
        open(&files, BUFFER);
        let publication = order.begin_buffer(URI, None, &files).unwrap();
        files.write().remove(URI);
        std::thread::scope(|s| {
            let reread = s.spawn(|| order.begin_disk(URI, &files, true).is_some());
            order.block_until_waited_on();
            drop(publication);
            assert!(reread.join().unwrap());
        });
    }

    /// A buffer from before a close and reopen that is still publishing
    /// lands before the reopened one.
    #[test]
    fn a_buffer_waits_for_an_earlier_buffer_still_publishing() {
        let order = PublicationOrder::default();
        let files = RwLock::new(HashMap::new());
        open(&files, "<?php // before the close");
        let publication = order.begin_buffer(URI, None, &files).unwrap();
        std::thread::scope(|s| {
            let reopened = s.spawn(|| {
                open(&files, BUFFER);
                order.begin_buffer(URI, None, &files).is_some()
            });
            order.block_until_waited_on();
            drop(publication);
            assert!(reopened.join().unwrap());
        });
    }

    #[test]
    fn a_closed_files_buffer_does_not_publish() {
        let order = PublicationOrder::default();
        let files = RwLock::new(HashMap::new());
        assert!(order.begin_buffer(URI, None, &files).is_none());
    }

    /// A buffer an edit, or a close and reopen, has since replaced is not
    /// the buffer, even with the same text.
    #[test]
    fn a_replaced_buffer_does_not_publish() {
        let order = PublicationOrder::default();
        let files = RwLock::new(HashMap::new());
        open(&files, BUFFER);
        let replaced = Arc::clone(&files.read()[URI]);
        open(&files, BUFFER);
        assert!(order.begin_buffer(URI, Some(&replaced), &files).is_none());

        let current = Arc::clone(&files.read()[URI]);
        assert!(order.begin_buffer(URI, Some(&current), &files).is_some());
    }

    /// Waiting while holding a publication could wait on a thread that is
    /// waiting on this one.
    #[test]
    fn a_thread_holding_a_publication_does_not_wait() {
        let order = PublicationOrder::default();
        let files = RwLock::new(HashMap::new());
        let _disk = order.begin_disk(URI, &files, true).unwrap();
        open(&files, BUFFER);
        let _buffer = order.begin_buffer(URI, None, &files).unwrap();
        assert!(order.begin_buffer(URI, None, &files).is_some());
    }
}
