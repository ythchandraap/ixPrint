use std::sync::{Mutex, MutexGuard};

/// Locks `m`, recovering the data if a previous holder panicked.
/// Our shared state is always left consistent between statements, so a
/// poisoned lock must not take every later request down with it.
pub trait LockExt<T> {
    fn lock_safe(&self) -> MutexGuard<'_, T>;
}

impl<T> LockExt<T> for Mutex<T> {
    fn lock_safe(&self) -> MutexGuard<'_, T> {
        self.lock().unwrap_or_else(|e| e.into_inner())
    }
}

use std::sync::{RwLock, RwLockReadGuard, RwLockWriteGuard};

pub trait RwLockExt<T> {
    fn read_safe(&self) -> RwLockReadGuard<'_, T>;
    fn write_safe(&self) -> RwLockWriteGuard<'_, T>;
}

impl<T> RwLockExt<T> for RwLock<T> {
    fn read_safe(&self) -> RwLockReadGuard<'_, T> {
        self.read().unwrap_or_else(|e| e.into_inner())
    }

    fn write_safe(&self) -> RwLockWriteGuard<'_, T> {
        self.write().unwrap_or_else(|e| e.into_inner())
    }
}

#[cfg(test)]
mod tests {
    use super::LockExt;
    use std::sync::{Arc, Mutex};

    #[test]
    fn lock_safe_recovers_poisoned_mutex_from_another_thread() {
        let mutex = Arc::new(Mutex::new(42));
        let holder = Arc::clone(&mutex);
        let panic = std::thread::spawn(move || {
            let _guard = holder.lock().unwrap();
            panic!("poison the mutex");
        })
        .join();
        assert!(panic.is_err());

        let reader = Arc::clone(&mutex);
        let value = std::thread::spawn(move || *reader.lock_safe()).join().unwrap();
        assert_eq!(value, 42);
    }
}
