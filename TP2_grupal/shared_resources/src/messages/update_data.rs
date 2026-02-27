//! Archivo con estructuras y funciones relaciodas al mensaje de
//! actualización de propiedades que el administrador envía al SV central.

use crate::utils::{read_f64, read_string, read_u64, AccountId, CardId};
use std::io::Cursor;

pub const UPD_ACCOUNT_LIMIT: u8 = 2;
pub const UPD_ACCOUNT_NAME: u8 = 3;
pub const UPD_CARD: u8 = 4;

#[derive(Debug)]
/// Mensaje de actualización/modificación de propiedades
/// de la cuenta o una de sus terjatas.
pub struct UpdateDataMessage {
    id: AccountId,
    /// Nuevo nombre de la cuenta, si está vacío, se ignora el cambio.
    new_company_name: String,
    /// Si el id de tarjeta es nula, el nuevo límite aplica a la cuenta en sí. // TODO: verificar que el nuevo límite no rompa la prop de la sumatoria.
    card_id: CardId,
    /// Nuevo límite, si es nulo, se ignora el cambio.
    new_limit: f64,
}

impl UpdateDataMessage {
    pub fn new(id: AccountId, new_company_name: String, card_id: CardId, new_limit: f64) -> Self {
        Self {
            id,
            new_company_name,
            card_id,
            new_limit,
        }
    }

    pub fn new_name(&self) -> String {
        self.new_company_name.to_string()
    }

    pub fn account_id(&self) -> AccountId {
        self.id
    }

    pub fn card_id(&self) -> CardId {
        self.card_id
    }

    pub fn limit(&self) -> f64 {
        self.new_limit
    }

    /// Serializa el mensaje para poder enviarlo por un socket como un vector de bytes
    /// `[id_bytes, len_company_name_bytes, company_name_bytes, card_id_bytes, new_limit_bytes]`
    pub fn serialize(&self) -> Vec<u8> {
        let mut buf = Vec::new();

        let code = if !self.new_company_name.is_empty() {
            UPD_ACCOUNT_NAME
        } else if self.card_id == 0 {
            UPD_ACCOUNT_LIMIT
        } else {
            UPD_CARD
        };

        buf.extend_from_slice(&code.to_be_bytes());

        buf.extend_from_slice(&self.id.to_be_bytes());

        let name_bytes = self.new_company_name.as_bytes();
        buf.extend_from_slice(&(name_bytes.len() as u64).to_be_bytes());
        buf.extend_from_slice(name_bytes);

        buf.extend_from_slice(&self.card_id.to_be_bytes());
        buf.extend_from_slice(&self.new_limit.to_be_bytes());

        buf
    }

    /// Deserializa un `UpdateDataMessage` a partir de un vector de bytes.
    pub fn deserialize(mut bytes: &[u8]) -> Result<Self, String> {
        let mut cursor = Cursor::new(&mut bytes);

        let id = read_u64(&mut cursor)?;

        let name_len = read_u64(&mut cursor)? as usize;
        let new_company_name = read_string(&mut cursor, name_len)?;

        let card_id = read_u64(&mut cursor)?;
        let new_limit = read_f64(&mut cursor)?;

        Ok(Self {
            id,
            new_company_name,
            card_id,
            new_limit,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_creates_update_data_message() {
        let msg = UpdateDataMessage::new(12345, "New Company Name".to_string(), 67890, 50000.0);

        assert_eq!(msg.id, 12345);
        assert_eq!(msg.new_company_name, "New Company Name");
        assert_eq!(msg.card_id, 67890);
        assert_eq!(msg.new_limit, 50000.0);
    }

    #[test]
    fn test_serialize_produces_correct_structure() {
        let msg = UpdateDataMessage::new(100, "TestCo".to_string(), 200, 10000.0);

        let serialized = msg.serialize();

        // Structure: id(8) + name_len(8) + name(6) + card_id(8) + new_limit(8) = 38 bytes
        assert_eq!(serialized.len(), 39);

        // Verify id
        let id_bytes = &serialized[1..9];
        assert_eq!(u64::from_be_bytes(id_bytes.try_into().unwrap()), 100);

        // Verify name length
        let name_len_bytes = &serialized[9..17];
        assert_eq!(u64::from_be_bytes(name_len_bytes.try_into().unwrap()), 6);

        // Verify name
        let name_bytes = &serialized[17..23];
        assert_eq!(std::str::from_utf8(name_bytes).unwrap(), "TestCo");

        // Verify card_id
        let card_id_bytes = &serialized[23..31];
        assert_eq!(u64::from_be_bytes(card_id_bytes.try_into().unwrap()), 200);

        // Verify new_limit
        let limit_bytes = &serialized[31..39];
        assert_eq!(f64::from_be_bytes(limit_bytes.try_into().unwrap()), 10000.0);
    }

    #[test]
    fn test_deserialize_from_valid_bytes() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&100u64.to_be_bytes());

        let name = "TestCo";
        bytes.extend_from_slice(&(name.len() as u64).to_be_bytes());
        bytes.extend_from_slice(name.as_bytes());

        bytes.extend_from_slice(&200u64.to_be_bytes());
        bytes.extend_from_slice(&10000.0f64.to_be_bytes());

        let result = UpdateDataMessage::deserialize(&bytes);
        assert!(result.is_ok());

        let msg = result.unwrap();
        assert_eq!(msg.id, 100);
        assert_eq!(msg.new_company_name, "TestCo");
        assert_eq!(msg.card_id, 200);
        assert_eq!(msg.new_limit, 10000.0);
    }

    #[test]
    fn test_serialize_deserialize_roundtrip() {
        let original =
            UpdateDataMessage::new(12345, "My Company Name".to_string(), 67890, 100000.50);

        let serialized = original.serialize();
        let deserialized = UpdateDataMessage::deserialize(&serialized[1..]).unwrap();

        assert_eq!(original.id, deserialized.id);
        assert_eq!(original.new_company_name, deserialized.new_company_name);
        assert_eq!(original.card_id, deserialized.card_id);
        assert_eq!(original.new_limit, deserialized.new_limit);
    }

    #[test]
    fn test_with_empty_company_name() {
        // Empty string means "ignore the name change"
        let msg = UpdateDataMessage::new(100, "".to_string(), 200, 10000.0);

        let serialized = msg.serialize();
        let deserialized = UpdateDataMessage::deserialize(&serialized[1..]).unwrap();

        assert_eq!(deserialized.new_company_name, "");
    }

    #[test]
    fn test_empty_name_means_no_change() {
        // Test semantic: empty name should be ignored in actual usage
        let msg = UpdateDataMessage::new(100, "".to_string(), 200, 10000.0);

        // Serialization should still work correctly
        let serialized = msg.serialize();
        assert_eq!(serialized.len(), 1 + 8 + 8 + 0 + 8 + 8); // 32 bytes (no name bytes)
    }

    #[test]
    fn test_with_long_company_name() {
        let long_name = "A".repeat(256);
        let msg = UpdateDataMessage::new(100, long_name.clone(), 200, 10000.0);

        let serialized = msg.serialize();
        let deserialized = UpdateDataMessage::deserialize(&serialized[1..]).unwrap();

        assert_eq!(deserialized.new_company_name, long_name);
        assert_eq!(deserialized.new_company_name.len(), 256);
    }

    #[test]
    fn test_with_unicode_company_name() {
        let unicode_name = "Compañía Española 日本語 🏢".to_string();
        let msg = UpdateDataMessage::new(100, unicode_name.clone(), 200, 10000.0);

        let serialized = msg.serialize();
        let deserialized = UpdateDataMessage::deserialize(&serialized[1..]).unwrap();

        assert_eq!(deserialized.new_company_name, unicode_name);
    }

    #[test]
    fn test_with_zero_card_id_applies_to_account() {
        // card_id = 0 means the limit applies to the account itself
        let msg = UpdateDataMessage::new(100, "Account Update".to_string(), 0, 50000.0);

        let serialized = msg.serialize();
        let deserialized = UpdateDataMessage::deserialize(&serialized[1..]).unwrap();

        assert_eq!(deserialized.card_id, 0);
    }

    #[test]
    fn test_with_nonzero_card_id_applies_to_card() {
        // Non-zero card_id means the limit applies to specific card
        let msg = UpdateDataMessage::new(100, "Card Update".to_string(), 12345, 5000.0);

        let serialized = msg.serialize();
        let deserialized = UpdateDataMessage::deserialize(&serialized[1..]).unwrap();

        assert_eq!(deserialized.card_id, 12345);
        assert!(deserialized.card_id != 0);
    }

    #[test]
    fn test_with_zero_limit_means_no_change() {
        // new_limit = 0.0 should be treated as "ignore the change"
        let msg = UpdateDataMessage::new(100, "Test".to_string(), 200, 0.0);

        let serialized = msg.serialize();
        let deserialized = UpdateDataMessage::deserialize(&serialized[1..]).unwrap();

        assert_eq!(deserialized.new_limit, 0.0);
    }

    #[test]
    fn test_with_max_u64_values() {
        let msg = UpdateDataMessage::new(u64::MAX, "Max Values".to_string(), u64::MAX, f64::MAX);

        let serialized = msg.serialize();
        let deserialized = UpdateDataMessage::deserialize(&serialized[1..]).unwrap();

        assert_eq!(deserialized.id, u64::MAX);
        assert_eq!(deserialized.card_id, u64::MAX);
        assert_eq!(deserialized.new_limit, f64::MAX);
    }

    #[test]
    fn test_with_negative_limit() {
        // Negative limit (maybe for suspended accounts or special cases)
        let msg = UpdateDataMessage::new(100, "Suspended".to_string(), 200, -1000.0);

        let serialized = msg.serialize();
        let deserialized = UpdateDataMessage::deserialize(&serialized[1..]).unwrap();

        assert_eq!(deserialized.new_limit, -1000.0);
    }

    #[test]
    fn test_deserialize_with_insufficient_bytes() {
        let bytes = vec![0u8; 10]; // Too few bytes
        let result = UpdateDataMessage::deserialize(&bytes);

        assert!(result.is_err());
    }

    #[test]
    fn test_deserialize_with_empty_bytes() {
        let bytes: Vec<u8> = vec![];
        let result = UpdateDataMessage::deserialize(&bytes);

        assert!(result.is_err());
    }

    #[test]
    fn test_deserialize_with_partial_data() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&100u64.to_be_bytes());
        bytes.extend_from_slice(&6u64.to_be_bytes());
        // Missing name bytes and remaining fields

        let result = UpdateDataMessage::deserialize(&bytes);
        assert!(result.is_err());
    }

    #[test]
    fn test_deserialize_with_mismatched_name_length() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&100u64.to_be_bytes());
        bytes.extend_from_slice(&10u64.to_be_bytes()); // Claims 10 bytes
        bytes.extend_from_slice(b"Short"); // But only 5 bytes provided

        let result = UpdateDataMessage::deserialize(&bytes);
        assert!(result.is_err());
    }

    #[test]
    fn test_deserialize_extra_bytes_ignored() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&100u64.to_be_bytes());

        let name = "TestCo";
        bytes.extend_from_slice(&(name.len() as u64).to_be_bytes());
        bytes.extend_from_slice(name.as_bytes());

        bytes.extend_from_slice(&200u64.to_be_bytes());
        bytes.extend_from_slice(&10000.0f64.to_be_bytes());
        bytes.extend_from_slice(&[99, 99, 99]); // Extra bytes

        let result = UpdateDataMessage::deserialize(&bytes);
        assert!(result.is_ok());

        let msg = result.unwrap();
        assert_eq!(msg.id, 100);
    }

    #[test]
    fn test_serialize_consistency() {
        let msg = UpdateDataMessage::new(100, "Consistent".to_string(), 200, 10000.0);

        let serialized1 = msg.serialize();
        let serialized2 = msg.serialize();

        assert_eq!(serialized1, serialized2);
    }

    #[test]
    fn test_float_precision() {
        let msg = UpdateDataMessage::new(100, "Precision Test".to_string(), 200, 12345.6789);

        let serialized = msg.serialize();
        let deserialized = UpdateDataMessage::deserialize(&serialized[1..]).unwrap();

        assert_eq!(deserialized.new_limit, 12345.6789);
    }

    #[test]
    fn test_special_float_values() {
        let test_cases = vec![f64::INFINITY, f64::NEG_INFINITY, f64::MIN, f64::MAX];

        for special_value in test_cases {
            let msg = UpdateDataMessage::new(100, "Special Float".to_string(), 200, special_value);

            let serialized = msg.serialize();
            let deserialized = UpdateDataMessage::deserialize(&serialized[1..]).unwrap();

            assert_eq!(deserialized.new_limit, special_value);
        }
    }

    #[test]
    fn test_realistic_update_account_scenario() {
        // Scenario: Update account 12345 with new company name and limit
        let msg = UpdateDataMessage::new(
            12345,
            "Updated Corporation Ltd.".to_string(),
            0, // 0 indicates account-level update
            150000.0,
        );

        let serialized = msg.serialize();
        let deserialized = UpdateDataMessage::deserialize(&serialized[1..]).unwrap();

        assert_eq!(deserialized.id, 12345);
        assert_eq!(deserialized.new_company_name, "Updated Corporation Ltd.");
        assert_eq!(deserialized.card_id, 0);
        assert_eq!(deserialized.new_limit, 150000.0);
    }

    #[test]
    fn test_realistic_update_card_scenario() {
        // Scenario: Update card 67890 limit for account 12345
        let msg = UpdateDataMessage::new(12345, "Company Name".to_string(), 67890, 25000.0);

        let serialized = msg.serialize();
        let deserialized = UpdateDataMessage::deserialize(&serialized[1..]).unwrap();

        assert_eq!(deserialized.id, 12345);
        assert_eq!(deserialized.card_id, 67890);
        assert_eq!(deserialized.new_limit, 25000.0);
    }

    #[test]
    fn test_update_only_name_scenario() {
        // Update only the company name, keep limit unchanged (0.0 = ignore)
        let msg = UpdateDataMessage::new(12345, "New Company Name Only".to_string(), 0, 0.0);

        let serialized = msg.serialize();
        let deserialized = UpdateDataMessage::deserialize(&serialized[1..]).unwrap();

        assert_eq!(deserialized.new_company_name, "New Company Name Only");
        assert_eq!(deserialized.new_limit, 0.0);
    }

    #[test]
    fn test_update_only_limit_scenario() {
        // Update only the limit, keep name unchanged (empty = ignore)
        let msg = UpdateDataMessage::new(12345, "".to_string(), 0, 75000.0);

        let serialized = msg.serialize();
        let deserialized = UpdateDataMessage::deserialize(&serialized[1..]).unwrap();

        assert_eq!(deserialized.new_company_name, "");
        assert_eq!(deserialized.new_limit, 75000.0);
    }

    #[test]
    fn test_no_changes_scenario() {
        // Both empty name and zero limit = no changes
        let msg = UpdateDataMessage::new(12345, "".to_string(), 0, 0.0);

        let serialized = msg.serialize();
        let deserialized = UpdateDataMessage::deserialize(&serialized[1..]).unwrap();

        assert_eq!(deserialized.new_company_name, "");
        assert_eq!(deserialized.new_limit, 0.0);
    }

    #[test]
    fn test_multiple_updates_different_names() {
        let names = vec![
            "Company A",
            "Company B",
            "Very Long Company Name With Many Words",
            "短い名前",
            "Société Française",
        ];

        for name in names {
            let msg = UpdateDataMessage::new(100, name.to_string(), 200, 10000.0);

            let serialized = msg.serialize();
            let deserialized = UpdateDataMessage::deserialize(&serialized[1..]).unwrap();

            assert_eq!(deserialized.new_company_name, name);
        }
    }

    #[test]
    fn test_variable_size_based_on_name_length() {
        let msg_short = UpdateDataMessage::new(100, "A".to_string(), 200, 10000.0);
        let msg_long = UpdateDataMessage::new(100, "AAAAAAAAAA".to_string(), 200, 10000.0);

        let serialized_short = msg_short.serialize();
        let serialized_long = msg_long.serialize();

        // Difference should be 9 bytes (10 - 1)
        assert_eq!(serialized_long.len() - serialized_short.len(), 9);
    }

    #[test]
    fn test_increase_limit_scenario() {
        // Increasing credit limit
        let msg = UpdateDataMessage::new(12345, "Growing Business Inc.".to_string(), 0, 200000.0);

        let serialized = msg.serialize();
        let deserialized = UpdateDataMessage::deserialize(&serialized[1..]).unwrap();

        assert_eq!(deserialized.new_limit, 200000.0);
    }

    #[test]
    fn test_decrease_limit_scenario() {
        // Decreasing credit limit
        let msg = UpdateDataMessage::new(12345, "Risky Account".to_string(), 0, 5000.0);

        let serialized = msg.serialize();
        let deserialized = UpdateDataMessage::deserialize(&serialized[1..]).unwrap();

        assert_eq!(deserialized.new_limit, 5000.0);
    }

    #[test]
    fn test_company_name_with_special_characters() {
        let special_name = "Company & Co. (Ltd.) - 2024!".to_string();
        let msg = UpdateDataMessage::new(100, special_name.clone(), 200, 10000.0);

        let serialized = msg.serialize();
        let deserialized = UpdateDataMessage::deserialize(&serialized[1..]).unwrap();

        assert_eq!(deserialized.new_company_name, special_name);
    }

    #[test]
    fn test_company_name_with_whitespace() {
        let name_with_spaces = "  Company   With   Spaces  ".to_string();
        let msg = UpdateDataMessage::new(100, name_with_spaces.clone(), 200, 10000.0);

        let serialized = msg.serialize();
        let deserialized = UpdateDataMessage::deserialize(&serialized[1..]).unwrap();

        assert_eq!(deserialized.new_company_name, name_with_spaces);
    }

    #[test]
    fn test_byte_order_big_endian() {
        let msg = UpdateDataMessage::new(
            0x0102030405060708,
            "Test".to_string(),
            0x090A0B0C0D0E0F10,
            1000.0,
        );

        let serialized = msg.serialize();

        // Verify big-endian encoding of id
        let id_bytes = &serialized[1..9];
        assert_eq!(id_bytes[0], 0x01);
        assert_eq!(id_bytes[1], 0x02);

        // Verify big-endian encoding of card_id (after name)
        // Position: 8 (id) + 8 (name_len) + 4 (name) = 20
        let card_id_bytes = &serialized[21..29];
        assert_eq!(card_id_bytes[0], 0x09);
        assert_eq!(card_id_bytes[1], 0x0A);
    }

    #[test]
    fn test_multiple_cards_same_account() {
        // Update different cards for the same account
        let card_ids = vec![1001, 1002, 1003, 1004, 1005];

        for card_id in card_ids {
            let msg = UpdateDataMessage::new(12345, "Account Company".to_string(), card_id, 5000.0);

            let serialized = msg.serialize();
            let deserialized = UpdateDataMessage::deserialize(&serialized[1..]).unwrap();

            assert_eq!(deserialized.id, 12345);
            assert_eq!(deserialized.card_id, card_id);
        }
    }

    #[test]
    fn test_very_small_limit() {
        // Very small limit (cents)
        let msg = UpdateDataMessage::new(100, "Small Limit".to_string(), 200, 0.01);

        let serialized = msg.serialize();
        let deserialized = UpdateDataMessage::deserialize(&serialized[1..]).unwrap();

        assert_eq!(deserialized.new_limit, 0.01);
    }

    #[test]
    fn test_very_large_limit() {
        // Very large limit (millions)
        let msg = UpdateDataMessage::new(100, "Large Limit".to_string(), 200, 10_000_000.00);

        let serialized = msg.serialize();
        let deserialized = UpdateDataMessage::deserialize(&serialized[1..]).unwrap();

        assert_eq!(deserialized.new_limit, 10_000_000.00);
    }
}
