use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionMessage {
    pub transaction_id: Uuid,         // ID único generado por el surtidor
    pub account_id: u64,              // ID de la cuenta de la empresa
    pub card_id: u64,                 // ID de la tarjeta física (¡Necesario para validar límites!)
    pub amount: f64,                  // Monto total de la venta
    pub timestamp: u64,               // Fecha/Hora
    pub station_id: u64,              // (Opcional) Quién vendió
    pub pump_address: Option<String>, // Dirección del pump para respuestas (IP:PORT)
}
