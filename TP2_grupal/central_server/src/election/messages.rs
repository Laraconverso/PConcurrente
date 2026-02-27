//! Mensajes utilizados en el algoritmo de elección de líder basado en anillo.

use serde::{Deserialize, Serialize};

/// Información de un nodo en el cluster (serializable para mensajes).
/// `addr` es la dirección del puerto de anillo (para comunicación interna entre nodos)
/// `client_addr` es la dirección expuesta a clientes (ip:client_port)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NodeInfoMsg {
    pub id: u8,
    pub addr: String,
    pub client_addr: String,
}

/// Mensajes intercambiados entre nodos del anillo para elección de líder.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ElectionMessage {
    /// Mensaje de elección que circula por el anillo llevando el ID del candidato.
    /// El nodo con mayor ID se convierte en líder.
    /// También lleva una lista de nodos visitados para descubrimiento de topología.
    Election {
        candidate_id: u8,
        visited_nodes: Vec<NodeInfoMsg>, // Nodos visitados durante esta elección
    },

    /// Anuncio del líder electo que circula por el anillo.
    /// Todos los nodos actualizan su conocimiento del líder actual.
    /// Incluye la lista completa del cluster descubierta durante la elección.
    Elected {
        leader_id: u8,
        cluster_nodes: Vec<NodeInfoMsg>, // Lista completa del cluster
    },

    /// Heartbeat enviado periódicamente por el líder para demostrar que está vivo.
    Heartbeat { leader_id: u8, sequence: u64 },

    /// Sincronización de topología del cluster.
    /// Enviado a un nodo cuando se detecta su recuperación después de una caída.
    /// Permite que nodos recuperados obtengan la lista completa sin esperar a la próxima elección.
    ClusterSync {
        cluster_nodes: Vec<NodeInfoMsg>, // Lista completa actual del cluster
        leader_id: u8,                   // Líder actual
        origin_id: u8,                   // ID del nodo que inició el sync (para detección de ciclo)
        client_addr: String,
    },

    /// Mensaje de replicación de estado (cambios en cuentas).
    /// Enviado por el líder cuando procesa una operación de escritura.
    StateUpdate {
        operation_type: u8, // Tipo de operación (REG_ACCOUNT, UPD_LIMIT, etc.)
        account_id: u64,    // ID de la cuenta afectada
        data: Vec<u8>,      // Datos serializados del cambio
        timestamp: u64,     // Timestamp de la operación
    },

    /// Solicitud de sincronización completa de estado.
    /// Enviado por un nodo cuando se une/recupera y necesita el estado actual completo.
    StateSyncRequest { requesting_node_id: u8 },

    /// Respuesta con sincronización completa de estado.
    /// Enviado por el líder con todas las cuentas actuales.
    StateSyncResponse {
        accounts: Vec<u8>, // Todas las cuentas serializadas como JSON array
    },
}

/// Estructura para enviar actualizaciones de estado entre threads.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateUpdateData {
    pub operation_type: u8,
    pub account_id: u64,
    pub data: Vec<u8>,
    pub timestamp: u64,
}

impl ElectionMessage {
    /// Serializa el mensaje a JSON para enviarlo por la red.
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }

    /// Deserializa un mensaje desde JSON.
    pub fn from_json(json: &str) -> Result<Self, String> {
        serde_json::from_str(json).map_err(|e| e.to_string())
    }
}
