//! Archivo con la declaración de la estructura de mensaje `RegisterCardMessage`
//! Usada para dar aviso al SV central del alta de una nueva cuenta.

use crate::utils::{read_f64, read_u64, AccountId};
use std::io::Cursor;

pub const REG_CARD: u8 = 1;

/// Mensaje que transporta id de la compañía para dar de alta una
/// nueva tarjeta.
pub struct RegisterCardMessage {
    id: AccountId,
    limit: f64,
}

impl RegisterCardMessage {
    pub fn new(id: u64, limit: f64) -> Self {
        Self { id, limit }
    }

    pub fn account_id(&self) -> AccountId {
        self.id
    }

    pub fn limit(&self) -> f64 {
        self.limit
    }

    /// Produce un vector de bytes siguiendo formato
    /// `[REG_CARD][id_bytes][limit_bytes]`
    pub fn serialize(&self) -> Vec<u8> {
        let mut buf = Vec::new();

        buf.extend_from_slice(&REG_CARD.to_be_bytes());
        buf.extend_from_slice(&self.id.to_be_bytes());
        buf.extend_from_slice(&self.limit.to_be_bytes());

        buf
    }

    /// Deserialize un `RegisterCardMessage` a partir de un conjunto de bytes.
    pub fn deserialize(mut bytes: &[u8]) -> Result<Self, String> {
        let mut cursor = Cursor::new(&mut bytes);

        let id = read_u64(&mut cursor)?;
        let limit = read_f64(&mut cursor)?;

        Ok(Self { id, limit })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_creates_message_with_correct_id_and_limit() {
        let id = 12345u64;
        let limit = 5000.0;
        let message = RegisterCardMessage::new(id, limit);

        assert_eq!(message.id, id);
        assert_eq!(message.limit, limit);
    }

    #[test]
    fn test_account_id_getter() {
        let id = 12345u64;
        let limit = 5000.0;
        let message = RegisterCardMessage::new(id, limit);

        assert_eq!(message.account_id(), id);
    }

    #[test]
    fn test_limit_getter() {
        let id = 12345u64;
        let limit = 5000.0;
        let message = RegisterCardMessage::new(id, limit);

        assert_eq!(message.limit(), limit);
    }

    #[test]
    fn test_serialize_produces_correct_bytes() {
        let id = 0x0102030405060708u64;
        let limit = 1000.0;
        let message = RegisterCardMessage::new(id, limit);

        let serialized = message.serialize();

        // Should be 1 (REG_CARD) + 8 (u64) + 8 (f64) = 17 bytes
        assert_eq!(serialized.len(), 17);

        // Check that first byte is REG_CARD
        assert_eq!(serialized[0], REG_CARD);

        // Check id bytes (big-endian)
        assert_eq!(
            &serialized[1..9],
            &[0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08]
        );
    }

    #[test]
    fn test_serialize_with_zero_values() {
        let message = RegisterCardMessage::new(0, 0.0);
        let serialized = message.serialize();

        assert_eq!(serialized.len(), 17);
        assert_eq!(serialized[0], REG_CARD);
        // Rest should be zeros
        assert!(serialized[1..].iter().all(|&b| b == 0));
    }

    #[test]
    fn test_serialize_with_max_id() {
        let message = RegisterCardMessage::new(u64::MAX, 9999.99);
        let serialized = message.serialize();

        assert_eq!(serialized.len(), 17);
        assert_eq!(serialized[0], REG_CARD);
        assert_eq!(&serialized[1..9], &[0xFF; 8]);
    }

    #[test]
    fn test_serialize_with_negative_limit() {
        let message = RegisterCardMessage::new(123, -500.0);
        let serialized = message.serialize();

        assert_eq!(serialized.len(), 17);
        assert_eq!(serialized[0], REG_CARD);
    }

    #[test]
    fn test_deserialize_valid_bytes() {
        let mut bytes = vec![0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08];
        // Add 8 bytes for f64 (1000.0 in IEEE 754)
        bytes.extend_from_slice(&1000.0f64.to_be_bytes());

        let result = RegisterCardMessage::deserialize(&bytes);

        assert!(result.is_ok());
        let message = result.unwrap();
        assert_eq!(message.id, 0x0102030405060708u64);
        assert_eq!(message.limit, 1000.0);
    }

    #[test]
    fn test_deserialize_insufficient_bytes_id() {
        let bytes = vec![0x01, 0x02, 0x03]; // Only 3 bytes instead of 16

        let result = RegisterCardMessage::deserialize(&bytes);

        assert!(result.is_err());
    }

    #[test]
    fn test_deserialize_insufficient_bytes_limit() {
        let mut bytes = vec![0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08];
        bytes.extend_from_slice(&[0x00, 0x00]); // Only 2 bytes for limit instead of 8

        let result = RegisterCardMessage::deserialize(&bytes);

        assert!(result.is_err());
    }

    #[test]
    fn test_deserialize_empty_bytes() {
        let bytes = vec![];

        let result = RegisterCardMessage::deserialize(&bytes);

        assert!(result.is_err());
    }

    #[test]
    fn test_serialize_deserialize_roundtrip() {
        let original_id = 9876543210u64;
        let original_limit = 7500.50;
        let original_message = RegisterCardMessage::new(original_id, original_limit);

        let serialized = original_message.serialize();
        // Skip first byte (REG_CARD) for deserialization
        let deserialized = RegisterCardMessage::deserialize(&serialized[1..]).unwrap();

        assert_eq!(deserialized.id, original_id);
        assert_eq!(deserialized.limit, original_limit);
    }

    #[test]
    fn test_roundtrip_with_various_values() {
        let test_cases = vec![
            (0, 0.0),
            (1, 100.0),
            (255, 500.50),
            (256, 1000.0),
            (u64::MAX / 2, 50000.0),
            (u64::MAX - 1, 99999.99),
            (u64::MAX, f64::MAX),
        ];

        for (id, limit) in test_cases {
            let message = RegisterCardMessage::new(id, limit);
            let serialized = message.serialize();
            let deserialized = RegisterCardMessage::deserialize(&serialized[1..]).unwrap();

            assert_eq!(deserialized.id, id, "Roundtrip failed for id: {}", id);
            assert_eq!(
                deserialized.limit, limit,
                "Roundtrip failed for limit: {}",
                limit
            );
        }
    }

    #[test]
    fn test_deserialize_extra_bytes_ignored() {
        let mut bytes = vec![0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08];
        bytes.extend_from_slice(&1000.0f64.to_be_bytes());
        bytes.extend_from_slice(&[0x09, 0x0A]); // Extra bytes

        let result = RegisterCardMessage::deserialize(&bytes);

        // Should succeed and only read the first 16 bytes
        assert!(result.is_ok());
        let message = result.unwrap();
        assert_eq!(message.id, 0x0102030405060708u64);
        assert_eq!(message.limit, 1000.0);
    }

    #[test]
    fn test_serialize_with_fractional_limit() {
        let message = RegisterCardMessage::new(100, 1234.56);
        let serialized = message.serialize();
        let deserialized = RegisterCardMessage::deserialize(&serialized[1..]).unwrap();

        assert_eq!(deserialized.limit, 1234.56);
    }

    #[test]
    fn test_serialize_with_large_limit() {
        let message = RegisterCardMessage::new(200, 1_000_000.0);
        let serialized = message.serialize();
        let deserialized = RegisterCardMessage::deserialize(&serialized[1..]).unwrap();

        assert_eq!(deserialized.limit, 1_000_000.0);
    }

    #[test]
    fn test_special_float_values() {
        let test_limits = vec![f64::MIN, f64::MAX, 0.0, -0.0, f64::EPSILON];

        for limit in test_limits {
            let message = RegisterCardMessage::new(42, limit);
            let serialized = message.serialize();
            let deserialized = RegisterCardMessage::deserialize(&serialized[1..]).unwrap();

            assert_eq!(deserialized.limit, limit, "Failed for limit: {}", limit);
        }
    }
}
