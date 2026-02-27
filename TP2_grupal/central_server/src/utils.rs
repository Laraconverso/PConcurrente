use crate::database::account_data::AccountData;
use shared_resources::database::transaction::Transaction;
use shared_resources::utils::AccountId;
use std::collections::HashMap;

/// Base de datos de cuentas con índice secundario por username
pub struct AccountDB {
    accounts: HashMap<AccountId, AccountData>,
    username_index: HashMap<String, AccountId>,
}

impl AccountDB {
    pub fn new() -> Self {
        Self {
            accounts: HashMap::new(),
            username_index: HashMap::new(),
        }
    }

    /// Inserta una cuenta validando unicidad de username
    pub fn insert(&mut self, id: AccountId, account: AccountData) -> Result<(), String> {
        let username = account.username();

        if self.username_index.contains_key(&username) {
            return Err(format!("Username '{}' ya existe", username));
        }

        self.username_index.insert(username, id);
        self.accounts.insert(id, account);
        Ok(())
    }

    /// Actualiza o inserta una cuenta (upsert) - usado para replicación
    /// Si la cuenta existe, la reemplaza completamente
    pub fn upsert(&mut self, id: AccountId, account: AccountData) {
        let username = account.username();
        self.username_index.insert(username, id);
        self.accounts.insert(id, account);
    }

    /// Obtiene una cuenta por ID
    pub fn get(&self, id: &AccountId) -> Option<&AccountData> {
        self.accounts.get(id)
    }

    /// Obtiene una cuenta mutable por ID
    pub fn get_mut(&mut self, id: &AccountId) -> Option<&mut AccountData> {
        self.accounts.get_mut(id)
    }

    /// Verifica si existe una cuenta con este ID
    pub fn contains_key(&self, id: &AccountId) -> bool {
        self.accounts.contains_key(id)
    }

    /// Itera sobre todas las cuentas
    pub fn iter(&self) -> std::collections::hash_map::Iter<'_, AccountId, AccountData> {
        self.accounts.iter()
    }
}

pub type TransactionDB = HashMap<AccountId, Vec<Transaction>>;
