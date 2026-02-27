use shared_resources::messages::transaction::TransactionMessage;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

/// Type alias para la cola de bulks pendientes
pub type BulkQueue = Arc<Mutex<VecDeque<(u64, Vec<TransactionMessage>)>>>;
pub use std::collections::VecDeque as VecDequeReExport;

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn test_bulk_queue_creation() {
        let queue: BulkQueue = Arc::new(Mutex::new(VecDeque::new()));
        let lock = queue.lock().unwrap();
        assert_eq!(lock.len(), 0);
    }

    #[test]
    fn test_bulk_queue_push_and_pop() {
        let queue: BulkQueue = Arc::new(Mutex::new(VecDeque::new()));
        
        let transaction = TransactionMessage {
            transaction_id: Uuid::new_v4(),
            account_id: 1,
            card_id: 100,
            amount: 500.0,
            timestamp: 123456,
            station_id: 10,
            pump_address: None,
        };
        
        {
            let mut lock = queue.lock().unwrap();
            lock.push_back((1, vec![transaction.clone()]));
        }
        
        let mut lock = queue.lock().unwrap();
        assert_eq!(lock.len(), 1);
        
        let (bulk_id, transactions) = lock.pop_front().unwrap();
        assert_eq!(bulk_id, 1);
        assert_eq!(transactions.len(), 1);
        assert_eq!(transactions[0].account_id, 1);
    }

    #[test]
    fn test_bulk_queue_fifo_order() {
        let queue: BulkQueue = Arc::new(Mutex::new(VecDeque::new()));
        
        {
            let mut lock = queue.lock().unwrap();
            lock.push_back((1, vec![]));
            lock.push_back((2, vec![]));
            lock.push_back((3, vec![]));
        }
        
        let mut lock = queue.lock().unwrap();
        assert_eq!(lock.pop_front().unwrap().0, 1);
        assert_eq!(lock.pop_front().unwrap().0, 2);
        assert_eq!(lock.pop_front().unwrap().0, 3);
    }

    #[test]
    fn test_bulk_queue_concurrent_access() {
        use std::thread;
        
        let queue: BulkQueue = Arc::new(Mutex::new(VecDeque::new()));
        
        let handles: Vec<_> = (0..10)
            .map(|i| {
                let q = Arc::clone(&queue);
                thread::spawn(move || {
                    let mut lock = q.lock().unwrap();
                    lock.push_back((i, vec![]));
                })
            })
            .collect();
        
        for handle in handles {
            handle.join().unwrap();
        }
        
        let lock = queue.lock().unwrap();
        assert_eq!(lock.len(), 10);
    }
}
