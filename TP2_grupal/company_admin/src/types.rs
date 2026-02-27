use std::sync::{Arc, Condvar, Mutex};

/// Type alias para el tipo complejo de sincronización usado en el admin
pub type SyncPair = Arc<(Mutex<(bool, Option<u64>)>, Condvar)>;

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn test_sync_pair_creation() {
        let pair: SyncPair = Arc::new((Mutex::new((false, None)), Condvar::new()));
        let (lock, _) = &*pair;
        let data = lock.lock().unwrap();
        assert_eq!(*data, (false, None));
    }

    #[test]
    fn test_sync_pair_update_state() {
        let pair: SyncPair = Arc::new((Mutex::new((false, None)), Condvar::new()));
        
        {
            let (lock, _) = &*pair;
            let mut data = lock.lock().unwrap();
            *data = (true, Some(12345));
        }
        
        let (lock, _) = &*pair;
        let data = lock.lock().unwrap();
        assert_eq!(*data, (true, Some(12345)));
    }

    #[test]
    fn test_sync_pair_notify() {
        let pair: SyncPair = Arc::new((Mutex::new((false, None)), Condvar::new()));
        let pair_clone = Arc::clone(&pair);
        
        let handle = thread::spawn(move || {
            let (lock, cvar) = &*pair_clone;
            let mut data = lock.lock().unwrap();
            *data = (true, Some(999));
            cvar.notify_one();
        });
        
        // Esperar a que el thread notifique
        handle.join().unwrap();
        
        let (lock, _) = &*pair;
        let data = lock.lock().unwrap();
        assert_eq!(data.0, true);
        assert_eq!(data.1, Some(999));
    }

    #[test]
    fn test_sync_pair_wait_with_timeout() {
        let pair: SyncPair = Arc::new((Mutex::new((false, None)), Condvar::new()));
        
        let (lock, cvar) = &*pair;
        let data = lock.lock().unwrap();
        
        // Wait con timeout corto (no debería cambiar nada)
        let (new_data, timeout_result) = cvar
            .wait_timeout(data, Duration::from_millis(10))
            .unwrap();
        
        assert!(timeout_result.timed_out());
        assert_eq!(*new_data, (false, None));
    }
}
