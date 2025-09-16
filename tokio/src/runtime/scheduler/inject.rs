//! Inject queue used to send wakeups to a work-stealing scheduler

use std::iter::FusedIterator;
use std::marker::PhantomData;
use std::ptr::NonNull;

use crate::loom::sync::atomic::{self, AtomicBool, AtomicUsize};
use crate::runtime::task::{self, Header, RawTask};

use cordyceps::MpscQueue;

// mod shared;
// pub(crate) use shared::Shared;
//
// mod synced;
// pub(crate) use synced::Synced;

mod metrics;

/// Growable, MPMC queue used to inject new tasks into the scheduler and as an
/// overflow queue when the local, fixed-size, array queue overflows.
pub(crate) struct Inject<T: 'static> {
    /// True if the queue is closed.
    is_closed: AtomicBool,
    len: AtomicUsize,

    pub(super) queue: MpscQueue<Header>,

    // Needs to be _after_ the queue (so it's dropped later).
    // We don't use a real `Box<_>` here, because it has additional
    // aliasing requirements and we just need a deallocation.
    _dealloc_dummy: DeallocDummy,
    _phantom: PhantomData<T>,
}

struct DeallocDummy(*mut Header);

impl Drop for DeallocDummy {
    fn drop(&mut self) {
        drop(unsafe { Box::from_raw(self.0) });
    }
}

// trust me bro
unsafe impl<T: 'static> Send for Inject<T> {}
unsafe impl<T: 'static> Sync for Inject<T> {}

impl<T: 'static> Inject<T> {
    pub(crate) fn new() -> Inject<T> {
        // FIXME: leaks memory
        let (stub, _dealloc_dummy) = unsafe {
            let ptr = Box::into_raw(Box::new(Header::evil_unusable_dummy_header()));
            (
                RawTask::from_raw(NonNull::new_unchecked(ptr)),
                DeallocDummy(ptr),
            )
        };
        Inject {
            is_closed: AtomicBool::new(false),
            len: AtomicUsize::new(0),
            queue: MpscQueue::new_with_stub(stub),
            _dealloc_dummy,
            _phantom: PhantomData,
        }
    }

    // Kind of annoying to have to include the cfg here
    pub(crate) fn is_closed(&self) -> bool {
        // TODO: ordering here?
        self.is_closed.load(atomic::Ordering::Acquire)
    }

    /// Closes the injection queue, returns `true` if the queue is open when the
    /// transition is made.
    pub(crate) fn close(&self) -> bool {
        // TODO: ordering here?
        !self.is_closed.swap(true, atomic::Ordering::AcqRel)
    }

    pub(crate) fn len(&self) -> usize {
        self.len.load(atomic::Ordering::Acquire)
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Pushes a value into the queue.
    ///
    /// This does nothing if the queue is closed.
    pub(crate) fn push(&self, task: task::Notified<T>) {
        self.len.fetch_add(1, atomic::Ordering::AcqRel);
        self.queue.enqueue(task.into_raw());
    }

    pub(crate) fn pop(&self) -> Option<task::Notified<T>> {
        if let Some(item) = self.queue.dequeue() {
            self.len.fetch_sub(1, atomic::Ordering::AcqRel);
            Some(unsafe { task::Notified::from_raw(item) })
        } else {
            None
        }
    }

    pub(crate) fn pop_n(&self, n: usize) -> impl Iterator<Item = task::Notified<T>> + use<'_, T> {
        PopN { queue: self, n }
    }
}

struct PopN<'a, T: 'static> {
    queue: &'a Inject<T>,
    n: usize,
}

impl<'a, T: 'static> Iterator for PopN<'a, T> {
    type Item = task::Notified<T>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.n == 0 {
            None
        } else {
            let item = self.queue.pop();
            if item.is_some() {
                self.n -= 1;
            } else {
                // Fuse the iterator.
                self.n = 0;
            }
            item
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (0, Some(self.n))
    }
}

impl<'a, T: 'static> FusedIterator for PopN<'a, T> {}
