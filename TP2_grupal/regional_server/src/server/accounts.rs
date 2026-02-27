use crate::types::PendingCards;
use shared_resources::messages::protocol::TransactionStatus;
use shared_resources::messages::transaction::TransactionMessage;
use shared_resources::utils::get_timestamp;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, RwLock};

/// Log informativo con timestamp y componente
fn log_info(regional_id: u8, msg: &str) {
    println!(
        "[{}] [REGIONAL #{}] [INFO] {}",
        get_timestamp(),
        regional_id,
        msg
    );
}

#[derive(Debug, Clone)]
pub struct AccountState {
    pub username: String,     // Username para login
    pub display_name: String, // Nombre para mostrar
    pub limit: f64,
    pub current_spending: f64,
    pub reserved: f64,
    pub card_limits: HashMap<u64, f64>,
    pub active: bool,
}

impl AccountState {
    pub fn new(_account_id: u64, username: String, display_name: String, limit: f64) -> Self {
        Self {
            username,
            display_name,
            limit,
            current_spending: 0.0,
            reserved: 0.0,
            card_limits: HashMap::new(),
            active: true,
        }
    }

    pub fn available(&self) -> f64 {
        self.limit - (self.current_spending + self.reserved)
    }

    pub fn card_exists(&self, card_id: u64) -> bool {
        self.card_limits.contains_key(&card_id)
    }

    pub fn add_card(&mut self, card_id: u64, limit: f64) {
        self.card_limits.insert(card_id, limit);
    }
}

#[derive(Clone)]
pub struct AccountsManager {
    cache: Arc<RwLock<HashMap<u64, AccountState>>>,
    pending_bulks: Arc<Mutex<HashMap<u64, Vec<TransactionMessage>>>>,
    pending_cards: PendingCards, // account_id -> [(card_id, limit), ...]
}

impl AccountsManager {
    pub fn new() -> Self {
        Self {
            cache: Arc::new(RwLock::new(HashMap::new())),
            pending_bulks: Arc::new(Mutex::new(HashMap::new())),
            pending_cards: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Actualiza una cuenta incluyendo su estado activo/inactivo
    pub fn update_account_with_state(
        &self,
        account_id: u64,
        username: String,
        display_name: String,
        limit: f64,
        active: bool,
    ) {
        {
            let mut accounts = self.cache.write().unwrap();
            let entry = accounts.entry(account_id).or_insert_with(|| {
                AccountState::new(account_id, username.clone(), display_name.clone(), limit)
            });
            entry.username = username;
            entry.display_name = display_name;
            entry.limit = limit;
            entry.active = active;
        } // Liberar lock de accounts

        // Procesar tarjetas pendientes para esta cuenta
        let pending_cards = {
            let mut pending = self.pending_cards.lock().unwrap();
            pending.remove(&account_id)
        };

        if let Some(cards) = pending_cards {
            let mut accounts = self.cache.write().unwrap();
            if let Some(account) = accounts.get_mut(&account_id) {
                for (card_id, card_limit) in cards {
                    account.add_card(card_id, card_limit);
                }
            }
        }
    }

    /// Desactiva una cuenta (marca como inactiva)
    pub fn deactivate_account(&self, account_id: u64) {
        let mut accounts = self.cache.write().unwrap();
        if let Some(account) = accounts.get_mut(&account_id) {
            account.active = false;
        }
    }

    /// Elimina completamente una cuenta del caché (cuando se remueve la región)
    pub fn remove_account(&self, account_id: u64) {
        let mut accounts = self.cache.write().unwrap();
        accounts.remove(&account_id);
    }

    pub fn add_card_to_account(&self, account_id: u64, card_id: u64, card_limit: f64) {
        let mut accounts = self.cache.write().unwrap();
        if let Some(account) = accounts.get_mut(&account_id) {
            account.add_card(card_id, card_limit);
        } else {
            // Si la cuenta no existe todavía, guardar en pending_cards
            drop(accounts); // Liberar el lock antes de adquirir otro
            let mut pending = self.pending_cards.lock().unwrap();
            pending
                .entry(account_id)
                .or_default()
                .push((card_id, card_limit));
        }
    }

    pub fn reserve_batch(
        &self,
        bulk_id: u64,
        transactions: Vec<TransactionMessage>,
        regional_id: u8,
    ) -> (
        bool,
        Vec<TransactionMessage>,
        Vec<(TransactionMessage, TransactionStatus)>,
    ) {
        let mut accounts = self.cache.write().unwrap();
        let mut valid_transactions = Vec::new();
        let mut rejected_transactions = Vec::new();
        let mut needed_totals: HashMap<u64, f64> = HashMap::new();

        // Primera pasada: validar cada transacción individualmente
        for tx in transactions {
            let rejection_reason = if let Some(account) = accounts.get(&tx.account_id) {
                if !account.active {
                    Some(TransactionStatus::RejectedAccountInactive)
                } else if !account.card_exists(tx.card_id) {
                    Some(TransactionStatus::RejectedCardNotFound)
                } else if let Some(&card_limit) = account.card_limits.get(&tx.card_id) {
                    if tx.amount > card_limit {
                        Some(TransactionStatus::RejectedCardLimitExceeded {
                            card_limit,
                            attempted: tx.amount,
                        })
                    } else {
                        None // Válida hasta ahora
                    }
                } else {
                    Some(TransactionStatus::RejectedCardNotFound)
                }
            } else {
                Some(TransactionStatus::RejectedAccountNotFound)
            };

            if let Some(status) = rejection_reason {
                log_info(
                    regional_id,
                    &format!("TX {} rechazada: {:?}", tx.transaction_id, status),
                );
                rejected_transactions.push((tx, status));
            } else {
                valid_transactions.push(tx.clone());
                *needed_totals.entry(tx.account_id).or_insert(0.0) += tx.amount;
            }
        }

        // Si no hay transacciones válidas, VOTE_ABORT
        if valid_transactions.is_empty() {
            drop(accounts);
            return (false, vec![], rejected_transactions);
        }

        // Segunda pasada: validar totales acumulados por cuenta
        let mut final_valid = Vec::new();
        let mut final_rejected = rejected_transactions;
        let mut final_totals: HashMap<u64, f64> = HashMap::new();

        for tx in valid_transactions {
            let current_total = final_totals.get(&tx.account_id).copied().unwrap_or(0.0);
            let new_total = current_total + tx.amount;

            if let Some(account) = accounts.get(&tx.account_id) {
                let available = account.available();
                if available >= new_total {
                    let account_id = tx.account_id; // Guardar antes de mover
                    final_valid.push(tx);
                    final_totals.insert(account_id, new_total);
                } else {
                    log_info(
                        regional_id,
                        &format!(
                            "TX {} rechazada: suma total excede límite disponible",
                            tx.transaction_id
                        ),
                    );
                    let status = TransactionStatus::RejectedLimitExceeded {
                        available,
                        attempted: new_total,
                    };
                    final_rejected.push((tx, status));
                }
            }
        }

        // Si después del filtro fino no queda nada, VOTE_ABORT
        if final_valid.is_empty() {
            drop(accounts);
            return (false, vec![], final_rejected);
        }

        // Reservar fondos para las transacciones válidas
        for (acc_id, total) in &final_totals {
            if let Some(account) = accounts.get_mut(acc_id) {
                account.reserved += total;
            }
        }

        drop(accounts);

        // Guardar solo las transacciones válidas
        self.pending_bulks
            .lock()
            .unwrap()
            .insert(bulk_id, final_valid.clone());

        log_info(
            regional_id,
            &format!(
                "Bulk {}: {} aceptadas, {} rechazadas",
                bulk_id,
                final_valid.len(),
                final_rejected.len()
            ),
        );

        // VOTE_COMMIT si hay al menos una válida
        (true, final_valid, final_rejected)
    }

    pub fn commit_batch(&self, bulk_id: u64, _regional_id: u8) -> Option<Vec<TransactionMessage>> {
        let mut pendings = self.pending_bulks.lock().unwrap();

        if let Some(transactions) = pendings.remove(&bulk_id) {
            let mut accounts = self.cache.write().unwrap();
            let mut commit_totals = HashMap::new();

            for t in &transactions {
                *commit_totals.entry(t.account_id).or_insert(0.0) += t.amount;
            }

            for (acc_id, total) in commit_totals {
                if let Some(acc) = accounts.get_mut(&acc_id) {
                    acc.reserved -= total;
                    acc.current_spending += total;
                }
            }
            return Some(transactions);
        }
        None
    }

    pub fn rollback_batch(&self, bulk_id: u64, _regional_id: u8) {
        let mut pendings = self.pending_bulks.lock().unwrap();

        if let Some(transactions) = pendings.remove(&bulk_id) {
            let mut accounts = self.cache.write().unwrap();
            let mut rollback_totals = HashMap::new();

            for t in &transactions {
                *rollback_totals.entry(t.account_id).or_insert(0.0) += t.amount;
            }

            for (acc_id, total) in rollback_totals {
                if let Some(acc) = accounts.get_mut(&acc_id) {
                    acc.reserved -= total;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_account_state_new() {
        let account = AccountState::new(1, "user1".to_string(), "User One".to_string(), 1000.0);
        
        assert_eq!(account.username, "user1");
        assert_eq!(account.display_name, "User One");
        assert_eq!(account.limit, 1000.0);
        assert_eq!(account.current_spending, 0.0);
        assert_eq!(account.reserved, 0.0);
        assert!(account.active);
        assert_eq!(account.card_limits.len(), 0);
    }

    #[test]
    fn test_account_state_available() {
        let mut account = AccountState::new(1, "user1".to_string(), "User One".to_string(), 1000.0);
        
        // Disponible inicial
        assert_eq!(account.available(), 1000.0);
        
        // Después de gastar
        account.current_spending = 300.0;
        assert_eq!(account.available(), 700.0);
        
        // Después de reservar
        account.reserved = 200.0;
        assert_eq!(account.available(), 500.0);
    }

    #[test]
    fn test_account_state_add_card() {
        let mut account = AccountState::new(1, "user1".to_string(), "User One".to_string(), 1000.0);
        
        account.add_card(100, 500.0);
        account.add_card(200, 300.0);
        
        assert_eq!(account.card_limits.len(), 2);
        assert_eq!(account.card_limits.get(&100), Some(&500.0));
        assert_eq!(account.card_limits.get(&200), Some(&300.0));
    }

    #[test]
    fn test_account_state_card_exists() {
        let mut account = AccountState::new(1, "user1".to_string(), "User One".to_string(), 1000.0);
        
        assert!(!account.card_exists(100));
        
        account.add_card(100, 500.0);
        assert!(account.card_exists(100));
        assert!(!account.card_exists(200));
    }

    #[test]
    fn test_accounts_manager_new() {
        let manager = AccountsManager::new();
        let accounts = manager.cache.read().unwrap();
        
        assert_eq!(accounts.len(), 0);
    }

    #[test]
    fn test_accounts_manager_update_account_with_state() {
        let manager = AccountsManager::new();
        
        manager.update_account_with_state(
            1,
            "user1".to_string(),
            "User One".to_string(),
            1000.0,
            true,
        );
        
        let accounts = manager.cache.read().unwrap();
        let account = accounts.get(&1).unwrap();
        
        assert_eq!(account.username, "user1");
        assert_eq!(account.display_name, "User One");
        assert_eq!(account.limit, 1000.0);
        assert!(account.active);
    }

    #[test]
    fn test_accounts_manager_add_card_to_account() {
        let manager = AccountsManager::new();
        
        // Primero crear la cuenta
        manager.update_account_with_state(
            1,
            "user1".to_string(),
            "User One".to_string(),
            1000.0,
            true,
        );
        
        // Agregar tarjeta
        manager.add_card_to_account(1, 100, 500.0);
        
        let accounts = manager.cache.read().unwrap();
        let account = accounts.get(&1).unwrap();
        
        assert!(account.card_exists(100));
        assert_eq!(account.card_limits.get(&100), Some(&500.0));
    }

    #[test]
    fn test_accounts_manager_add_card_to_pending_account() {
        let manager = AccountsManager::new();
        
        // Agregar tarjeta a cuenta que aún no existe
        manager.add_card_to_account(1, 100, 500.0);
        
        // Verificar que está en pending_cards
        let pending = manager.pending_cards.lock().unwrap();
        assert!(pending.contains_key(&1));
        let cards = pending.get(&1).unwrap();
        assert_eq!(cards.len(), 1);
        assert_eq!(cards[0], (100, 500.0));
    }

    #[test]
    fn test_accounts_manager_concurrent_read_access() {
        use std::thread;
        
        let manager = AccountsManager::new();
        manager.update_account_with_state(
            1,
            "user1".to_string(),
            "User One".to_string(),
            1000.0,
            true,
        );
        
        // Múltiples lecturas concurrentes (RwLock permite esto)
        let handles: Vec<_> = (0..10)
            .map(|_| {
                let mgr = manager.clone();
                thread::spawn(move || {
                    let accounts = mgr.cache.read().unwrap();
                    accounts.get(&1).is_some()
                })
            })
            .collect();
        
        for handle in handles {
            assert!(handle.join().unwrap());
        }
    }

    #[test]
    fn test_accounts_manager_reserve_and_confirm() {
        let manager = AccountsManager::new();
        
        manager.update_account_with_state(
            1,
            "user1".to_string(),
            "User One".to_string(),
            1000.0,
            true,
        );
        manager.add_card_to_account(1, 100, 500.0);
        
        // Reservar fondos
        {
            let mut accounts = manager.cache.write().unwrap();
            let account = accounts.get_mut(&1).unwrap();
            account.reserved = 300.0;
        }
        
        // Verificar reserva
        {
            let accounts = manager.cache.read().unwrap();
            let account = accounts.get(&1).unwrap();
            assert_eq!(account.reserved, 300.0);
            assert_eq!(account.available(), 700.0);
        }
        
        // Confirmar (pasar de reserved a current_spending)
        {
            let mut accounts = manager.cache.write().unwrap();
            let account = accounts.get_mut(&1).unwrap();
            account.current_spending += account.reserved;
            account.reserved = 0.0;
        }
        
        // Verificar confirmación
        {
            let accounts = manager.cache.read().unwrap();
            let account = accounts.get(&1).unwrap();
            assert_eq!(account.current_spending, 300.0);
            assert_eq!(account.reserved, 0.0);
            assert_eq!(account.available(), 700.0);
        }
    }

    #[test]
    fn test_accounts_manager_remove_account() {
        let manager = AccountsManager::new();
        
        manager.update_account_with_state(
            1,
            "user1".to_string(),
            "User One".to_string(),
            1000.0,
            true,
        );
        
        assert_eq!(manager.cache.read().unwrap().len(), 1);
        
        manager.remove_account(1);
        
        assert_eq!(manager.cache.read().unwrap().len(), 0);
    }
}
