//! Mensaje para remover regiones de una cuenta

use crate::utils::{read_string, read_u64, AccountId};
use std::io::Cursor;

pub const UPD_REMOVE_REGIONS: u8 = 10;

#[derive(Debug)]
/// Mensaje para remover regiones de una cuenta
pub struct RemoveRegionsMessage {
    account_id: AccountId,
    /// Lista de nombres de regiones a remover
    regions: Vec<String>,
}

impl RemoveRegionsMessage {
    pub fn new(account_id: AccountId, regions: Vec<String>) -> Self {
        Self {
            account_id,
            regions,
        }
    }

    pub fn account_id(&self) -> AccountId {
        self.account_id
    }

    pub fn regions(&self) -> &Vec<String> {
        &self.regions
    }

    /// Serializa el mensaje:
    /// [code(1), account_id(8), num_regions(8), [region_len(8), region_bytes], ...]
    pub fn serialize(&self) -> Vec<u8> {
        let mut buf = Vec::new();

        buf.extend_from_slice(&UPD_REMOVE_REGIONS.to_be_bytes());
        buf.extend_from_slice(&self.account_id.to_be_bytes());
        buf.extend_from_slice(&(self.regions.len() as u64).to_be_bytes());

        for region in &self.regions {
            let region_bytes = region.as_bytes();
            buf.extend_from_slice(&(region_bytes.len() as u64).to_be_bytes());
            buf.extend_from_slice(region_bytes);
        }

        buf
    }

    /// Deserializa un `RemoveRegionsMessage` a partir de un vector de bytes.
    pub fn deserialize(mut bytes: &[u8]) -> Result<Self, String> {
        let mut cursor = Cursor::new(&mut bytes);

        let account_id = read_u64(&mut cursor)?;
        let num_regions = read_u64(&mut cursor)? as usize;

        let mut regions = Vec::new();

        for _ in 0..num_regions {
            let region_len = read_u64(&mut cursor)? as usize;
            let region_name = read_string(&mut cursor, region_len)?;
            regions.push(region_name);
        }

        Ok(Self {
            account_id,
            regions,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_creates_correct_structure() {
        let account_id = 123456;
        let regions = vec!["Norte".to_string(), "Sur".to_string()];

        let msg = RemoveRegionsMessage::new(account_id, regions.clone());

        assert_eq!(msg.account_id(), account_id);
        assert_eq!(msg.regions(), &regions);
    }

    #[test]
    fn test_serialize_structure_and_order() {
        let account_id: u64 = 1;
        let region_name = "Test";
        let regions = vec![region_name.to_string()];
        let msg = RemoveRegionsMessage::new(account_id, regions);

        let serialized = msg.serialize();

        // Estructura esperada:
        // 1 byte:  OpCode (10)
        // 8 bytes: Account ID
        // 8 bytes: Num Regions (1)
        // 8 bytes: Region Len (4)
        // 4 bytes: "Test"
        // Total: 1 + 8 + 8 + 8 + 4 = 29 bytes

        assert_eq!(serialized.len(), 29);
        assert_eq!(serialized[0], UPD_REMOVE_REGIONS); // Byte 0 es el opcode

        // Verificamos Account ID (Big Endian)
        let acc_bytes = &serialized[1..9];
        assert_eq!(u64::from_be_bytes(acc_bytes.try_into().unwrap()), 1);

        // Verificamos Num Regions
        let num_reg_bytes = &serialized[9..17];
        assert_eq!(u64::from_be_bytes(num_reg_bytes.try_into().unwrap()), 1);

        // Verificamos Longitud de la string
        let len_str_bytes = &serialized[17..25];
        assert_eq!(u64::from_be_bytes(len_str_bytes.try_into().unwrap()), 4);

        // Verificamos el contenido de la string
        assert_eq!(&serialized[25..29], region_name.as_bytes());
    }

    #[test]
    fn test_roundtrip_skipping_opcode() {
        // ESTE ES EL CASO CRÍTICO:
        // Serializamos -> Quitamos el primer byte -> Deserializamos

        let regions = vec!["Latam-South".to_string(), "Europe-West".to_string()];
        let original = RemoveRegionsMessage::new(999, regions.clone());

        let serialized = original.serialize();

        // Simulamos la lectura del socket: quitamos el primer byte (opcode)
        let payload_without_opcode = &serialized[1..];

        let deserialized = RemoveRegionsMessage::deserialize(payload_without_opcode)
            .expect("Should deserialize correctly");

        assert_eq!(original.account_id(), deserialized.account_id());
        assert_eq!(original.regions(), deserialized.regions());
    }

    #[test]
    fn test_empty_regions_list() {
        let msg = RemoveRegionsMessage::new(555, Vec::new());

        let serialized = msg.serialize();

        // OpCode(1) + Account(8) + NumRegions(8) = 17 bytes
        assert_eq!(serialized.len(), 17);
        assert_eq!(serialized[0], UPD_REMOVE_REGIONS);

        // Deserialize sin opcode
        let deserialized =
            RemoveRegionsMessage::deserialize(&serialized[1..]).expect("Should handle empty list");

        assert_eq!(deserialized.regions().len(), 0);
        assert_eq!(deserialized.account_id(), 555);
    }

    #[test]
    fn test_special_characters_utf8() {
        let regions = vec!["España".to_string(), "Japón".to_string(), "💰".to_string()];
        let msg = RemoveRegionsMessage::new(100, regions.clone());

        let serialized = msg.serialize();
        let deserialized = RemoveRegionsMessage::deserialize(&serialized[1..])
            .expect("Should handle UTF-8 strings");

        assert_eq!(deserialized.regions(), &regions);
        assert_eq!(deserialized.regions()[0], "España");
    }

    #[test]
    fn test_deserialize_error_incomplete_data() {
        let msg = RemoveRegionsMessage::new(1, vec!["Region".to_string()]);
        let serialized = msg.serialize();

        // Cortamos el vector para que esté corrupto (quitamos el byte de opcode y unos bytes del final)
        // El payload debería tener: Account(8) + Num(8) + Len(8) + "Region"(6) = 30 bytes
        // Le damos menos
        let corrupted_payload = &serialized[1..15];

        let result = RemoveRegionsMessage::deserialize(corrupted_payload);

        assert!(result.is_err(), "Should fail on incomplete data");
    }

    #[test]
    fn test_deserialize_error_string_length_mismatch() {
        // Construimos un buffer inválido manualmente
        let mut buf = Vec::new();
        buf.extend_from_slice(&1u64.to_be_bytes()); // Account ID
        buf.extend_from_slice(&1u64.to_be_bytes()); // Num regions: 1
        buf.extend_from_slice(&100u64.to_be_bytes()); // String len: 100 bytes (mentira)
        buf.extend_from_slice("Short".as_bytes()); // Solo damos 5 bytes

        let result = RemoveRegionsMessage::deserialize(&buf);

        assert!(
            result.is_err(),
            "Should fail when string buffer is shorter than declared length"
        );
    }
}
