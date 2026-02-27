//! Mensaje para agregar/actualizar regiones habilitadas en una cuenta

use crate::utils::{read_f64, read_string, read_u64, AccountId};
use std::io::Cursor;

pub const UPD_REGIONS: u8 = 9;

#[derive(Debug)]
/// Mensaje para agregar o actualizar regiones con sus límites en una cuenta
pub struct UpdateRegionsMessage {
    account_id: AccountId,
    /// Lista de nombres de regiones
    regions: Vec<String>,
    /// Lista de límites correspondientes a cada región
    limits: Vec<f64>,
}

impl UpdateRegionsMessage {
    pub fn new(account_id: AccountId, regions: Vec<String>, limits: Vec<f64>) -> Self {
        assert_eq!(
            regions.len(),
            limits.len(),
            "Regions and limits must have the same length"
        );
        Self {
            account_id,
            regions,
            limits,
        }
    }

    pub fn account_id(&self) -> AccountId {
        self.account_id
    }

    pub fn regions(&self) -> &Vec<String> {
        &self.regions
    }

    pub fn limits(&self) -> &Vec<f64> {
        &self.limits
    }

    /// Serializa el mensaje:
    /// [code(1), account_id(8), num_regions(8), [region_len(8), region_bytes, limit(8)], ...]
    pub fn serialize(&self) -> Vec<u8> {
        let mut buf = Vec::new();

        buf.extend_from_slice(&UPD_REGIONS.to_be_bytes());
        buf.extend_from_slice(&self.account_id.to_be_bytes());
        buf.extend_from_slice(&(self.regions.len() as u64).to_be_bytes());

        for i in 0..self.regions.len() {
            let region_bytes = self.regions[i].as_bytes();
            buf.extend_from_slice(&(region_bytes.len() as u64).to_be_bytes());
            buf.extend_from_slice(region_bytes);
            buf.extend_from_slice(&self.limits[i].to_be_bytes());
        }

        buf
    }

    /// Deserializa un `UpdateRegionsMessage` a partir de un vector de bytes.
    pub fn deserialize(mut bytes: &[u8]) -> Result<Self, String> {
        let mut cursor = Cursor::new(&mut bytes);

        let account_id = read_u64(&mut cursor)?;
        let num_regions = read_u64(&mut cursor)? as usize;

        let mut regions = Vec::new();
        let mut limits = Vec::new();

        for _ in 0..num_regions {
            let region_len = read_u64(&mut cursor)? as usize;
            let region_name = read_string(&mut cursor, region_len)?;
            let limit = read_f64(&mut cursor)?;

            regions.push(region_name);
            limits.push(limit);
        }

        Ok(Self {
            account_id,
            regions,
            limits,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_creates_correct_structure() {
        let account_id = 123;
        let regions = vec!["Norte".to_string(), "Sur".to_string()];
        let limits = vec![1000.0, 500.50];

        let msg = UpdateRegionsMessage::new(account_id, regions.clone(), limits.clone());

        assert_eq!(msg.account_id(), account_id);
        assert_eq!(msg.regions(), &regions);
        assert_eq!(msg.limits(), &limits);
    }

    #[test]
    #[should_panic(expected = "Regions and limits must have the same length")]
    fn test_new_panics_on_mismatched_lengths() {
        let regions = vec!["Norte".to_string()];
        let limits = vec![100.0, 200.0]; // Dos límites, una región -> Panic

        UpdateRegionsMessage::new(1, regions, limits);
    }

    #[test]
    fn test_serialize_structure_and_order() {
        let account_id: u64 = 5;
        let region_name = "Test";
        let limit = 123.456;

        let msg = UpdateRegionsMessage::new(account_id, vec![region_name.to_string()], vec![limit]);

        let serialized = msg.serialize();

        // Estructura esperada:
        // 1 byte:  OpCode (9)
        // 8 bytes: Account ID
        // 8 bytes: Num Regions (1)
        // 8 bytes: Region Len (4)
        // 4 bytes: "Test"
        // 8 bytes: Limit (f64)
        // Total: 1 + 8 + 8 + 8 + 4 + 8 = 37 bytes

        assert_eq!(serialized.len(), 37);
        assert_eq!(serialized[0], UPD_REGIONS); // Byte 0 es el opcode

        // Verificamos Account ID
        let acc_bytes = &serialized[1..9];
        assert_eq!(u64::from_be_bytes(acc_bytes.try_into().unwrap()), 5);

        // Verificamos Num Regions
        let num_reg_bytes = &serialized[9..17];
        assert_eq!(u64::from_be_bytes(num_reg_bytes.try_into().unwrap()), 1);

        // Verificamos Longitud de la string
        let len_str_bytes = &serialized[17..25];
        assert_eq!(u64::from_be_bytes(len_str_bytes.try_into().unwrap()), 4);

        // Verificamos contenido de string
        assert_eq!(&serialized[25..29], region_name.as_bytes());

        // Verificamos el límite
        let limit_bytes = &serialized[29..37];
        assert_eq!(f64::from_be_bytes(limit_bytes.try_into().unwrap()), limit);
    }

    #[test]
    fn test_roundtrip_skipping_opcode() {
        // CASO CRÍTICO: Serializamos -> Quitamos byte 0 -> Deserializamos

        let regions = vec!["A".to_string(), "B".to_string()];
        let limits = vec![10.0, 20.0];
        let original = UpdateRegionsMessage::new(777, regions.clone(), limits.clone());

        let serialized = original.serialize();

        // Quitamos el opcode manual para simular lectura de socket
        let payload_without_opcode = &serialized[1..];

        let deserialized = UpdateRegionsMessage::deserialize(payload_without_opcode)
            .expect("Should deserialize correctly");

        assert_eq!(original.account_id(), deserialized.account_id());
        assert_eq!(original.regions(), deserialized.regions());
        assert_eq!(original.limits(), deserialized.limits());
    }

    #[test]
    fn test_empty_lists() {
        let msg = UpdateRegionsMessage::new(1, Vec::new(), Vec::new());
        let serialized = msg.serialize();

        // OpCode(1) + Account(8) + NumRegions(8) = 17 bytes
        assert_eq!(serialized.len(), 17);

        let deserialized =
            UpdateRegionsMessage::deserialize(&serialized[1..]).expect("Should handle empty lists");

        assert_eq!(deserialized.regions().len(), 0);
        assert_eq!(deserialized.limits().len(), 0);
    }

    #[test]
    fn test_deserialize_error_incomplete_payload() {
        let msg = UpdateRegionsMessage::new(1, vec!["X".to_string()], vec![1.0]);
        let serialized = msg.serialize();

        // El mensaje completo (sin opcode) debería ser:
        // Account(8) + Num(8) + Len(8) + "X"(1) + Limit(8) = 33 bytes

        // Pasamos un slice cortado justo antes del límite
        let corrupted_payload = &serialized[1..25];

        let result = UpdateRegionsMessage::deserialize(corrupted_payload);
        assert!(result.is_err(), "Should fail when float data is missing");
    }

    #[test]
    fn test_multiple_regions_order_preserved() {
        let regions = vec!["Uno".to_string(), "Dos".to_string(), "Tres".to_string()];
        let limits = vec![1.1, 2.2, 3.3];

        let msg = UpdateRegionsMessage::new(1, regions, limits);
        let serialized = msg.serialize();

        let deserialized = UpdateRegionsMessage::deserialize(&serialized[1..]).unwrap();

        assert_eq!(deserialized.regions()[0], "Uno");
        assert_eq!(deserialized.limits()[0], 1.1);

        assert_eq!(deserialized.regions()[2], "Tres");
        assert_eq!(deserialized.limits()[2], 3.3);
    }
}
