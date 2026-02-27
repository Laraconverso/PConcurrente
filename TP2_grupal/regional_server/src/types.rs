use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Type alias para las tarjetas pendientes por cuenta
pub type PendingCards = Arc<Mutex<HashMap<u64, Vec<(u64, f64)>>>>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pending_cards_creation() {
        let pending: PendingCards = Arc::new(Mutex::new(HashMap::new()));
        let lock = pending.lock().unwrap();
        assert_eq!(lock.len(), 0);
    }

    #[test]
    fn test_pending_cards_insert_and_retrieve() {
        let pending: PendingCards = Arc::new(Mutex::new(HashMap::new()));
        
        {
            let mut lock = pending.lock().unwrap();
            lock.insert(1, vec![(100, 500.0), (200, 300.0)]);
        }
        
        let lock = pending.lock().unwrap();
        assert_eq!(lock.len(), 1);
        assert_eq!(lock.get(&1), Some(&vec![(100, 500.0), (200, 300.0)]));
    }

    #[test]
    fn test_pending_cards_concurrent_access() {
        use std::thread;
        
        let pending: PendingCards = Arc::new(Mutex::new(HashMap::new()));
        
        let handles: Vec<_> = (0..10)
            .map(|i| {
                let p = Arc::clone(&pending);
                thread::spawn(move || {
                    let mut lock = p.lock().unwrap();
                    lock.insert(i, vec![(i * 10, 100.0)]);
                })
            })
            .collect();
        
        for handle in handles {
            handle.join().unwrap();
        }
        
        let lock = pending.lock().unwrap();
        assert_eq!(lock.len(), 10);
    }
}
