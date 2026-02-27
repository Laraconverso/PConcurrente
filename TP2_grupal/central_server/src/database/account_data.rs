use serde::{Deserialize, Serialize};
use shared_resources::utils::{CardId, FAILURE, GenId, SUCCESS};
use std::collections::HashMap;

#[derive(Debug, Serialize, Deserialize)]
pub struct AccountData {
    username: String,     // Inmutable, único, usado para login
    display_name: String, // Mutable, nombre para mostrar
    id: u64,
    enabled_regions: HashMap<String, f64>, // region_name -> limit en esa región
    state: bool, // True si la cuenta no se eliminó. TODO: Eliminar si no se contempla al entregar!
    cards: HashMap<CardId, f64>,
}

impl AccountData {
    pub fn new(username: String, display_name: String, id: u64) -> Self {
        Self {
            username,
            display_name,
            id,
            enabled_regions: HashMap::new(), // Arranca sin regiones habilitadas
            state: true,
            cards: HashMap::new(),
        }
    }

    /// Agrega una región con su límite
    pub fn add_region(&mut self, region_name: String, limit: f64) {
        self.enabled_regions.insert(region_name, limit);
    }

    /// Remueve una región
    pub fn remove_region(&mut self, region_name: &str) -> bool {
        self.enabled_regions.remove(region_name).is_some()
    }

    /// Obtiene el límite de una región específica
    pub fn get_region_limit(&self, region_name: &str) -> Option<f64> {
        self.enabled_regions.get(region_name).copied()
    }

    /// Obtiene todas las regiones habilitadas
    pub fn enabled_regions(&self) -> &HashMap<String, f64> {
        &self.enabled_regions
    }

    pub fn id(&self) -> u64 {
        self.id
    }

    pub fn username(&self) -> String {
        self.username.clone()
    }

    pub fn name(&self) -> String {
        self.display_name.clone()
    }

    pub fn display_name(&self) -> String {
        self.display_name.clone()
    }

    /// Obtiene el límite máximo entre todas las regiones (para validaciones de tarjetas)
    pub fn max_limit(&self) -> f64 {
        self.enabled_regions.values().copied().fold(0.0, f64::max)
    }

    pub fn state(&self) -> bool {
        self.state
    }

    pub fn cards(&self) -> HashMap<CardId, f64> {
        self.cards.clone()
    }

    pub fn register_card(&mut self, limit: f64) -> (CardId, u8) {
        let max_limit = self.max_limit();
        if !verify_limit_change(max_limit, &self.cards, limit) {
            return (0, FAILURE);
        }

        // Generar card_id sin región (random completo)
        let card_id = generate_card_id((self.cards.len() + 1) as u64);

        self.cards.insert(card_id, limit);

        (card_id, SUCCESS)
    }

    pub fn change_limit(&mut self, new_limit: f64, card_id: CardId) -> (u8, GenId) {
        if card_id == 0 {
            // No se puede cambiar límite de cuenta, solo de tarjetas
            // (el límite está por región ahora)
            return (FAILURE, self.id);
        }
        // TODO: Revisar que hacer, si esperar al próximo período (ver como hacer que tenga validez) o cambiar ahora (var también esa validez)
        if let Some(current_limit) = self.cards.get(&card_id).copied() {
            let diff = new_limit - current_limit;
            let max_limit = self.max_limit();

            if verify_limit_change(max_limit, &self.cards, diff) {
                *self.cards.get_mut(&card_id).unwrap() = new_limit;
                return (SUCCESS, card_id);
            }
        }
        (FAILURE, card_id)
    }

    pub fn change_name(&mut self, new_name: String) {
        self.display_name = new_name;
    }
}

/// Genera un card_id sin región (ahora las cuentas pueden estar en múltiples regiones)
fn generate_card_id(suffix: u64) -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static CARD_COUNTER: AtomicU64 = AtomicU64::new(1);

    let counter = CARD_COUNTER.fetch_add(1, Ordering::SeqCst);
    (counter << 32) | (suffix & 0xFFFFFFFF)
}

fn verify_limit_change(threshold: f64, cards: &HashMap<CardId, f64>, dif: f64) -> bool {
    let mut pot_total = dif;
    for limit in cards.values() {
        pot_total += limit;
    }
    pot_total <= threshold
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_account_data_new() {
        let account = AccountData::new("testuser".to_string(), "Test User".to_string(), 1);

        assert_eq!(account.username(), "testuser");
        assert_eq!(account.display_name(), "Test User");
        assert_eq!(account.id(), 1);
        assert_eq!(account.enabled_regions().len(), 0);
        assert!(account.state());
        assert_eq!(account.cards().len(), 0);
    }

    #[test]
    fn test_add_region() {
        let mut account = AccountData::new("testuser".to_string(), "Test User".to_string(), 1);

        account.add_region("CABA".to_string(), 1000.0);
        assert_eq!(account.enabled_regions().len(), 1);
        assert_eq!(account.get_region_limit("CABA"), Some(1000.0));
    }

    #[test]
    fn test_add_multiple_regions() {
        let mut account = AccountData::new("testuser".to_string(), "Test User".to_string(), 1);

        account.add_region("CABA".to_string(), 1000.0);
        account.add_region("Cordoba".to_string(), 1500.0);
        account.add_region("Mendoza".to_string(), 2000.0);

        assert_eq!(account.enabled_regions().len(), 3);
        assert_eq!(account.get_region_limit("CABA"), Some(1000.0));
        assert_eq!(account.get_region_limit("Cordoba"), Some(1500.0));
        assert_eq!(account.get_region_limit("Mendoza"), Some(2000.0));
    }

    #[test]
    fn test_remove_region() {
        let mut account = AccountData::new("testuser".to_string(), "Test User".to_string(), 1);

        account.add_region("CABA".to_string(), 1000.0);
        assert_eq!(account.enabled_regions().len(), 1);

        let removed = account.remove_region("CABA");
        assert!(removed);
        assert_eq!(account.enabled_regions().len(), 0);
        assert_eq!(account.get_region_limit("CABA"), None);
    }

    #[test]
    fn test_remove_non_existent_region() {
        let mut account = AccountData::new("testuser".to_string(), "Test User".to_string(), 1);

        let removed = account.remove_region("Inexistente");
        assert!(!removed);
    }

    #[test]
    fn test_max_limit() {
        let mut account = AccountData::new("testuser".to_string(), "Test User".to_string(), 1);

        assert_eq!(account.max_limit(), 0.0);

        account.add_region("CABA".to_string(), 1000.0);
        assert_eq!(account.max_limit(), 1000.0);

        account.add_region("Cordoba".to_string(), 1500.0);
        assert_eq!(account.max_limit(), 1500.0);

        account.add_region("Mendoza".to_string(), 800.0);
        assert_eq!(account.max_limit(), 1500.0);
    }

    #[test]
    fn test_register_card_success() {
        let mut account = AccountData::new("testuser".to_string(), "Test User".to_string(), 1);

        account.add_region("CABA".to_string(), 1000.0);

        let (card_id, status) = account.register_card(500.0);

        assert_eq!(status, SUCCESS);
        assert_ne!(card_id, 0);
        assert_eq!(account.cards().len(), 1);
        assert_eq!(account.cards().get(&card_id), Some(&500.0));
    }

    #[test]
    fn test_register_card_exceeds_limit() {
        let mut account = AccountData::new("testuser".to_string(), "Test User".to_string(), 1);

        account.add_region("CABA".to_string(), 1000.0);

        // Intentar registrar tarjeta con límite mayor al máximo
        let (card_id, status) = account.register_card(2000.0);

        assert_eq!(status, FAILURE);
        assert_eq!(card_id, 0);
        assert_eq!(account.cards().len(), 0);
    }

    #[test]
    fn test_register_multiple_cards() {
        let mut account = AccountData::new("testuser".to_string(), "Test User".to_string(), 1);

        account.add_region("CABA".to_string(), 2000.0);

        let (card_id1, status1) = account.register_card(500.0);
        let (card_id2, status2) = account.register_card(800.0);

        assert_eq!(status1, SUCCESS);
        assert_eq!(status2, SUCCESS);
        assert_ne!(card_id1, card_id2);
        assert_eq!(account.cards().len(), 2);
    }

    #[test]
    fn test_change_card_limit_success() {
        let mut account = AccountData::new("testuser".to_string(), "Test User".to_string(), 1);

        account.add_region("CABA".to_string(), 2000.0);

        let (card_id, _) = account.register_card(500.0);
        let (status, _) = account.change_limit(800.0, card_id);

        assert_eq!(status, SUCCESS);
        assert_eq!(account.cards().get(&card_id), Some(&800.0));
    }

    #[test]
    fn test_change_card_limit_non_existent() {
        let mut account = AccountData::new("testuser".to_string(), "Test User".to_string(), 1);

        account.add_region("CABA".to_string(), 2000.0);

        let (status, _) = account.change_limit(800.0, 99999);

        assert_eq!(status, FAILURE);
    }

    #[test]
    fn test_verify_limit_change_valid() {
        let mut cards = HashMap::new();
        cards.insert(1, 500.0);
        cards.insert(2, 300.0);

        // Límite total actual: 800.0, máximo permitido: 1000.0
        // Agregar tarjeta de 200.0 = 1000.0 total (OK)
        assert!(verify_limit_change(1000.0, &cards, 200.0));
    }

    #[test]
    fn test_verify_limit_change_exceeds() {
        let mut cards = HashMap::new();
        cards.insert(1, 500.0);
        cards.insert(2, 300.0);

        // Límite total actual: 800.0, máximo permitido: 1000.0
        // Agregar tarjeta de 300.0 = 1100.0 total (FALLA)
        assert!(!verify_limit_change(1000.0, &cards, 300.0));
    }

    #[test]
    fn test_verify_limit_change_exact() {
        let mut cards = HashMap::new();
        cards.insert(1, 500.0);

        // Límite total actual: 500.0, máximo permitido: 1000.0
        // Agregar tarjeta de 500.0 = 1000.0 total (OK - exacto)
        assert!(verify_limit_change(1000.0, &cards, 500.0));
    }
}
