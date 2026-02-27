use crate::messages::transaction::TransactionMessage;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Código de operación para mensajes de batch desde Regional Server a Central Server
pub const REGIONAL_BATCH: u8 = 6;

/// Código de operación para registro de Regional Server en Central Server
pub const REGISTER_REGIONAL: u8 = 8;

/// Mensajes de sincronización desde Central Server hacia Regional Servers
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CentralSyncMessage {
    /// Notifica creación o actualización de cuenta (por región)
    AccountUpdate {
        account_id: u64,
        username: String,     // Username para login
        display_name: String, // Nombre para mostrar
        region_name: String,  // Nombre de la región que recibe este update
        region_limit: f64,    // Límite específico para esta región
        active: bool,
    },

    /// Notifica creación o actualización de tarjeta
    CardUpdate {
        account_id: u64,
        card_id: u64,
        card_limit: f64,
    },

    /// Notifica eliminación de cuenta
    AccountDeleted { account_id: u64 },

    /// Sincronización completa al inicio (Regional pide todos sus datos)
    FullSync { region_id: u8 },

    /// Respuesta de sincronización completa
    FullSyncResponse { accounts: Vec<AccountSyncData> },

    /// Remover cuenta del regional (cuando se quita la región de la cuenta)
    RemoveAccount {
        account_id: u64,
        region_name: String, // Nombre de la región que debe remover esta cuenta
    },
}

/// Datos de una cuenta para sincronización completa
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountSyncData {
    pub account_id: u64,
    pub username: String,     // Username para login
    pub display_name: String, // Nombre para mostrar
    pub limit: f64,
    pub active: bool,
    pub cards: Vec<(u64, f64)>, // (card_id, card_limit)
}

/// Mensaje de registro de un Regional Server al Central Server
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterRegionalMessage {
    pub name: String,         // Nombre único alfanumérico de la región
    pub sync_address: String, // "ip:puerto" donde escucha actualizaciones del Central
}

impl RegisterRegionalMessage {
    pub fn new(name: String, sync_address: String) -> Self {
        Self { name, sync_address }
    }
}

/// Respuesta del Central Server al registro de un Regional
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterRegionalResponse {
    pub success: bool,
    pub region_id: u8,                  // ID único asignado a la región
    pub accounts: Vec<AccountSyncData>, // Cuentas iniciales de esta región
    pub error_message: Option<String>,  // Mensaje de error si success=false
    pub available_regions: Vec<String>, // Lista de regiones disponibles
    pub leader_address: Option<String>, // Dirección del líder si este nodo no es líder
}

impl RegisterRegionalResponse {
    pub fn success(
        region_id: u8,
        accounts: Vec<AccountSyncData>,
        available_regions: Vec<String>,
    ) -> Self {
        Self {
            success: true,
            region_id,
            accounts,
            error_message: None,
            available_regions,
            leader_address: None,
        }
    }

    pub fn error(error_message: String, available_regions: Vec<String>) -> Self {
        Self {
            success: false,
            region_id: 0,
            accounts: Vec::new(),
            error_message: Some(error_message),
            available_regions,
            leader_address: None,
        }
    }

    /// Respuesta de redirección cuando el nodo no es líder
    pub fn redirect(leader_address: String) -> Self {
        Self {
            success: false,
            region_id: 0,
            accounts: Vec::new(),
            error_message: Some(format!(
                "Este nodo no es el líder. Reintenta con: {}",
                leader_address
            )),
            available_regions: Vec::new(),
            leader_address: Some(leader_address),
        }
    }
}

/// Estado de una transacción después de validación
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TransactionStatus {
    Accepted,
    RejectedLimitExceeded { available: f64, attempted: f64 },
    RejectedCardLimitExceeded { card_limit: f64, attempted: f64 },
    RejectedAccountNotFound,
    RejectedCardNotFound,
    RejectedAccountInactive,
}

/// Respuesta individual de transacción para el pump
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionResponse {
    pub transaction_id: Uuid,
    pub status: TransactionStatus,
    pub message: String,
}

/// Mensajes del protocolo entre Gas Station y Regional Server
#[derive(Debug, Serialize, Deserialize)]
pub enum RegionalProtocolMessage {
    BulkTransaction {
        bulk_id: u64,
        transactions: Vec<TransactionMessage>,
    },

    BulkPrepareResponse {
        bulk_id: u64,
        vote: bool,
        accepted_transactions: Vec<TransactionMessage>, // TXs que pasaron validación
        rejected_transactions: Vec<(TransactionMessage, TransactionStatus)>, // TXs rechazadas con motivo
    },

    GlobalCommit {
        bulk_id: u64,
    },

    GlobalAbort {
        bulk_id: u64,
    },

    BulkCommittedAck {
        bulk_id: u64,
    },
}

/// Mensajes entre Regional Server y Central Server
#[derive(Debug, Serialize, Deserialize)]
pub struct RegionalBatchMessage {
    pub regional_id: u8,
    pub transactions: Vec<TransactionMessage>,
}

/// Mensaje de respuesta de Gas Station a Pump (se envía después del 2PC)
#[derive(Debug, Serialize, Deserialize)]
pub struct PumpResponseMessage {
    pub responses: Vec<TransactionResponse>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json;

    // --- Tests de Constantes ---

    #[test]
    fn test_constants_values() {
        assert_eq!(REGIONAL_BATCH, 6);
        assert_eq!(REGISTER_REGIONAL, 8);
    }

    // --- Tests de RegisterRegionalMessage ---

    #[test]
    fn test_register_regional_message_new() {
        let name = "Norte".to_string();
        let address = "127.0.0.1:8080".to_string();

        let msg = RegisterRegionalMessage::new(name.clone(), address.clone());

        assert_eq!(msg.name, name);
        assert_eq!(msg.sync_address, address);
    }

    #[test]
    fn test_register_regional_message_serialization() {
        let msg = RegisterRegionalMessage::new("Sur".to_string(), "localhost:9090".to_string());

        let serialized = serde_json::to_string(&msg).expect("Falló serialización");
        let deserialized: RegisterRegionalMessage =
            serde_json::from_str(&serialized).expect("Falló deserialización");

        assert_eq!(msg.name, deserialized.name);
        assert_eq!(msg.sync_address, deserialized.sync_address);
    }

    // --- Tests de RegisterRegionalResponse ---

    #[test]
    fn test_register_regional_response_success_factory() {
        let accounts = vec![AccountSyncData {
            account_id: 1,
            username: "user1".to_string(),
            display_name: "User One".to_string(),
            limit: 1000.0,
            active: true,
            cards: vec![(10, 500.0)],
        }];
        let regions = vec!["Norte".to_string(), "Sur".to_string()];

        let response = RegisterRegionalResponse::success(1, accounts.clone(), regions.clone());

        assert!(response.success);
        assert_eq!(response.region_id, 1);
        assert_eq!(response.accounts.len(), 1);
        assert!(response.error_message.is_none());
        assert_eq!(response.available_regions, regions);
    }

    #[test]
    fn test_register_regional_response_error_factory() {
        let regions = vec!["Este".to_string()];
        let error_msg = "Nombre de región duplicado".to_string();

        let response = RegisterRegionalResponse::error(error_msg.clone(), regions.clone());

        assert!(!response.success);
        assert_eq!(response.region_id, 0); // Default en error
        assert!(response.accounts.is_empty());
        assert_eq!(response.error_message, Some(error_msg));
        assert_eq!(response.available_regions, regions);
    }

    // --- Tests de CentralSyncMessage ---

    #[test]
    fn test_central_sync_message_account_update_serialization() {
        let msg = CentralSyncMessage::AccountUpdate {
            account_id: 100,
            username: "test_user".to_string(),
            display_name: "Test User".to_string(),
            region_name: "Oeste".to_string(),
            region_limit: 5000.0,
            active: true,
        };

        let serialized = serde_json::to_string(&msg).unwrap();
        let deserialized: CentralSyncMessage = serde_json::from_str(&serialized).unwrap();

        if let CentralSyncMessage::AccountUpdate {
            account_id,
            username,
            ..
        } = deserialized
        {
            assert_eq!(account_id, 100);
            assert_eq!(username, "test_user");
        } else {
            panic!("Deserialización incorrecta de AccountUpdate");
        }
    }

    #[test]
    fn test_central_sync_message_full_sync() {
        let msg = CentralSyncMessage::FullSync { region_id: 5 };

        // Verificamos Clone
        let cloned_msg = msg.clone();

        if let CentralSyncMessage::FullSync { region_id } = cloned_msg {
            assert_eq!(region_id, 5);
        } else {
            panic!("Falló el clone o match de FullSync");
        }
    }

    // --- Tests de TransactionStatus ---

    #[test]
    fn test_transaction_status_variants() {
        let _accepted = TransactionStatus::Accepted;
        let rejected_limit = TransactionStatus::RejectedLimitExceeded {
            available: 10.0,
            attempted: 50.0,
        };

        // Verificamos serialización de variantes con datos
        let serialized = serde_json::to_string(&rejected_limit).unwrap();
        assert!(serialized.contains("RejectedLimitExceeded"));
        assert!(serialized.contains("available"));
    }

    // --- Tests de TransactionResponse ---

    #[test]
    fn test_transaction_response_structure() {
        let uuid = Uuid::new_v4();
        let response = TransactionResponse {
            transaction_id: uuid,
            status: TransactionStatus::RejectedAccountInactive,
            message: "Cuenta inactiva".to_string(),
        };

        assert_eq!(response.transaction_id, uuid);
        assert_eq!(response.message, "Cuenta inactiva");

        match response.status {
            TransactionStatus::RejectedAccountInactive => assert!(true),
            _ => panic!("Status incorrecto"),
        }
    }

    // --- Tests de RegionalProtocolMessage (Protocolo 2PC) ---

    #[test]
    fn test_regional_protocol_message_bulk_prepare_response() {
        // Mock de transacciones actualizado con la estructura real
        let tx1 = TransactionMessage {
            transaction_id: Uuid::new_v4(),
            account_id: 1,
            card_id: 101,
            amount: 100.0,
            timestamp: 123456789,
            station_id: 10,
            pump_address: Some("127.0.0.1:5000".to_string()),
        };
        let tx2 = TransactionMessage {
            transaction_id: Uuid::new_v4(),
            account_id: 1,
            card_id: 102,
            amount: 200.0,
            timestamp: 123456799,
            station_id: 10,
            pump_address: None,
        };

        let msg = RegionalProtocolMessage::BulkPrepareResponse {
            bulk_id: 99,
            vote: false, // Voto negativo
            accepted_transactions: vec![tx1],
            rejected_transactions: vec![(
                tx2,
                TransactionStatus::RejectedCardLimitExceeded {
                    card_limit: 100.0,
                    attempted: 200.0,
                },
            )],
        };

        // Verificamos serialización compleja
        let serialized = serde_json::to_string(&msg).unwrap();
        let deserialized: RegionalProtocolMessage = serde_json::from_str(&serialized).unwrap();

        if let RegionalProtocolMessage::BulkPrepareResponse {
            bulk_id,
            vote,
            rejected_transactions,
            ..
        } = deserialized
        {
            assert_eq!(bulk_id, 99);
            assert_eq!(vote, false);
            assert_eq!(rejected_transactions.len(), 1);

            // Verificamos que se preservó el status de rechazo
            let (_, status) = &rejected_transactions[0];
            match status {
                TransactionStatus::RejectedCardLimitExceeded { card_limit, .. } => {
                    assert_eq!(*card_limit, 100.0)
                }
                _ => panic!("Status de rechazo incorrecto"),
            }
        } else {
            panic!("Deserialización incorrecta de BulkPrepareResponse");
        }
    }

    #[test]
    fn test_global_commit_message() {
        let msg = RegionalProtocolMessage::GlobalCommit { bulk_id: 12345 };

        match msg {
            RegionalProtocolMessage::GlobalCommit { bulk_id } => assert_eq!(bulk_id, 12345),
            _ => panic!("Tipo de mensaje incorrecto"),
        }
    }

    // --- Tests de RegionalBatchMessage ---

    #[test]
    fn test_regional_batch_message() {
        let txs = vec![
            TransactionMessage {
                transaction_id: Uuid::new_v4(),
                account_id: 1,
                card_id: 50,
                amount: 50.0,
                timestamp: 1000,
                station_id: 1,
                pump_address: None,
            },
            TransactionMessage {
                transaction_id: Uuid::new_v4(),
                account_id: 2,
                card_id: 60,
                amount: 20.0,
                timestamp: 1001,
                station_id: 2,
                pump_address: None,
            },
        ];

        let msg = RegionalBatchMessage {
            regional_id: 7,
            transactions: txs,
        };

        assert_eq!(msg.regional_id, 7);
        assert_eq!(msg.transactions.len(), 2);
        assert_eq!(msg.transactions[0].account_id, 1);
    }

    // --- Tests de PumpResponseMessage ---

    #[test]
    fn test_pump_response_message() {
        let responses = vec![TransactionResponse {
            transaction_id: Uuid::new_v4(),
            status: TransactionStatus::Accepted,
            message: "OK".to_string(),
        }];

        let pump_msg = PumpResponseMessage { responses };

        assert_eq!(pump_msg.responses.len(), 1);

        // Verificación de serialización
        let serialized = serde_json::to_string(&pump_msg).expect("Error serializando PumpResponse");
        assert!(serialized.contains("Accepted"));
    }
}
