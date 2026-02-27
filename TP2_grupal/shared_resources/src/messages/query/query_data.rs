//! Archivo con la estructura relacionada con el
//! mensaje de consulta para el servidor central.

use crate::utils::{read_timestamp, read_u64, read_u8, AccountId, CardId, TimeStamp};
use std::io::Cursor;

pub const QUERY: u8 = 5;

#[derive(Debug)]
/// Mensaje que transporta la información de la cuenta
/// a consultar, también con los filtros:
/// - card_id, filtrado por tarjeta. Si se deja en 0, se ven los records
///   de todas las tarjetas asociadas a la cuenta.
/// - start_date y end_date, filtrado por fechas. Si se dejan en 0, se ven los datos históricos.
pub struct QueryDataMessage {
    q_type: u8,
    id: AccountId,         // Id de la cuenta
    card_id: CardId,       // 0 si la query es para la cuenta
    start_date: TimeStamp, // Ambos 0 si busco históricos
    end_date: TimeStamp,
}

impl QueryDataMessage {
    pub fn new(
        q_type: u8,
        id: AccountId,
        card_id: CardId,
        start_date: TimeStamp,
        end_date: TimeStamp,
    ) -> Self {
        Self {
            q_type,
            id,
            card_id,
            start_date,
            end_date,
        }
    }

    pub fn account_id(&self) -> AccountId {
        self.id
    }

    pub fn card_id(&self) -> CardId {
        self.card_id
    }

    pub fn start_date(&self) -> TimeStamp {
        self.start_date
    }

    pub fn end_date(&self) -> TimeStamp {
        self.end_date
    }

    pub fn query_type(&self) -> u8 {
        self.q_type
    }

    /// Serializa siguiendo el formato `[id_bytes, card_id_bytes, start_date_bytes, end_date_bytes]`.
    /// Valores numéricos son serializados en big endian.
    pub fn serialize(&self) -> Vec<u8> {
        let mut buf = Vec::new();

        buf.push(QUERY);
        buf.push(self.q_type);
        buf.extend_from_slice(&self.id.to_be_bytes());
        buf.extend_from_slice(&self.card_id.to_be_bytes());
        buf.extend_from_slice(&self.start_date.to_be_bytes());
        buf.extend_from_slice(&self.end_date.to_be_bytes());

        buf
    }

    /// Deserializa un `QueryDataMessage` a partir de un conjunto de bytes.
    pub fn deserialize(mut bytes: &[u8]) -> Result<Self, String> {
        let mut cursor = Cursor::new(&mut bytes);

        let q_type = read_u8(&mut cursor)?;
        let id = read_u64(&mut cursor)?;
        let card_id = read_u64(&mut cursor)?;
        let start_date = read_timestamp(&mut cursor)?;
        let end_date = read_timestamp(&mut cursor)?;

        Ok(Self {
            q_type,
            id,
            card_id,
            start_date,
            end_date,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_creates_query_account_data() {
        let query = QueryDataMessage::new(0, 1, 2, 1000, 2000);

        assert_eq!(query.q_type, 0);
        assert_eq!(query.id, 1);
        assert_eq!(query.card_id, 2);
        assert_eq!(query.start_date, 1000);
        assert_eq!(query.end_date, 2000);
    }

    #[test]
    fn test_serialize_produces_correct_bytes() {
        let query = QueryDataMessage::new(0, 1, 2, 1000, 2000);
        let serialized = query.serialize();

        // Expected: 1 byte QUERY (message type) + 1 byte q_type + 8 bytes id + 8 bytes card_id + 8 bytes start_date + 8 bytes end_date = 34 bytes
        assert_eq!(serialized.len(), 34);

        // Verify QUERY constant (message type identifier)
        assert_eq!(serialized[0], QUERY);

        // Verify q_type
        assert_eq!(serialized[1], 0);

        // Verify each field in big-endian format
        let id_bytes = &serialized[2..10];
        let card_id_bytes = &serialized[10..18];
        let start_date_bytes = &serialized[18..26];
        let end_date_bytes = &serialized[26..34];

        assert_eq!(u64::from_be_bytes(id_bytes.try_into().unwrap()), 1);
        assert_eq!(u64::from_be_bytes(card_id_bytes.try_into().unwrap()), 2);
        assert_eq!(
            i64::from_be_bytes(start_date_bytes.try_into().unwrap()),
            1000
        );
        assert_eq!(i64::from_be_bytes(end_date_bytes.try_into().unwrap()), 2000);
    }

    #[test]
    fn test_deserialize_from_valid_bytes() {
        let mut bytes = Vec::new();
        bytes.push(0u8); // q_type
        bytes.extend_from_slice(&1u64.to_be_bytes());
        bytes.extend_from_slice(&2u64.to_be_bytes());
        bytes.extend_from_slice(&1000i64.to_be_bytes());
        bytes.extend_from_slice(&2000i64.to_be_bytes());

        let result = QueryDataMessage::deserialize(&bytes);
        assert!(result.is_ok());

        let query = result.unwrap();
        assert_eq!(query.q_type, 0);
        assert_eq!(query.id, 1);
        assert_eq!(query.card_id, 2);
        assert_eq!(query.start_date, 1000);
        assert_eq!(query.end_date, 2000);
    }

    #[test]
    fn test_serialize_deserialize_roundtrip() {
        let original = QueryDataMessage::new(1, 12345, 67890, 1234567890, 1987654321);
        let serialized = original.serialize();

        // Skip first byte (QUERY constant) for deserialize
        let deserialized = QueryDataMessage::deserialize(&serialized[1..]).unwrap();

        assert_eq!(original.q_type, deserialized.q_type);
        assert_eq!(original.id, deserialized.id);
        assert_eq!(original.card_id, deserialized.card_id);
        assert_eq!(original.start_date, deserialized.start_date);
        assert_eq!(original.end_date, deserialized.end_date);
    }

    #[test]
    fn test_realistic_timestamp_range() {
        // Realistic timestamp range (e.g., Unix timestamps for year 2024-2025)
        let start = 1704067200i64; // Jan 1, 2024
        let end = 1735689600i64; // Jan 1, 2025

        let query = QueryDataMessage::new(2, 999, 1001, start, end);
        let serialized = query.serialize();
        let deserialized = QueryDataMessage::deserialize(&serialized[1..]).unwrap();

        assert_eq!(deserialized.q_type, 2);
        assert_eq!(deserialized.start_date, start);
        assert_eq!(deserialized.end_date, end);
    }

    #[test]
    fn test_deserialize_with_zero_values() {
        let query = QueryDataMessage::new(0, 100, 0, 0, 0);
        let serialized = query.serialize();
        let deserialized = QueryDataMessage::deserialize(&serialized[1..]).unwrap();

        assert_eq!(deserialized.q_type, 0);
        assert_eq!(deserialized.id, 100);
        assert_eq!(deserialized.card_id, 0);
        assert_eq!(deserialized.start_date, 0);
        assert_eq!(deserialized.end_date, 0);
    }

    #[test]
    fn test_deserialize_with_max_values() {
        let query = QueryDataMessage::new(255, u64::MAX, u64::MAX, i64::MAX, i64::MAX);
        let serialized = query.serialize();
        let deserialized = QueryDataMessage::deserialize(&serialized[1..]).unwrap();

        assert_eq!(deserialized.q_type, 255);
        assert_eq!(deserialized.id, u64::MAX);
        assert_eq!(deserialized.card_id, u64::MAX);
        assert_eq!(deserialized.start_date, i64::MAX);
        assert_eq!(deserialized.end_date, i64::MAX);
    }

    #[test]
    fn test_deserialize_with_min_timestamp_values() {
        let query = QueryDataMessage::new(0, 100, 200, i64::MIN, i64::MIN);
        let serialized = query.serialize();
        let deserialized = QueryDataMessage::deserialize(&serialized[1..]).unwrap();

        assert_eq!(deserialized.id, 100);
        assert_eq!(deserialized.card_id, 200);
        assert_eq!(deserialized.start_date, i64::MIN);
        assert_eq!(deserialized.end_date, i64::MIN);
    }

    #[test]
    fn test_negative_timestamps() {
        // Negative timestamps (dates before epoch)
        let query = QueryDataMessage::new(0, 1, 2, -1000, -500);
        let serialized = query.serialize();
        let deserialized = QueryDataMessage::deserialize(&serialized[1..]).unwrap();

        assert_eq!(deserialized.start_date, -1000);
        assert_eq!(deserialized.end_date, -500);
    }

    #[test]
    fn test_deserialize_with_insufficient_bytes() {
        let bytes = vec![0u8; 16]; // Only 16 bytes instead of 33 (q_type + 4 u64/i64 fields)
        let result = QueryDataMessage::deserialize(&bytes);

        assert!(result.is_err());
    }

    #[test]
    fn test_deserialize_with_empty_bytes() {
        let bytes: Vec<u8> = vec![];
        let result = QueryDataMessage::deserialize(&bytes);

        assert!(result.is_err());
    }

    #[test]
    fn test_deserialize_with_partial_data() {
        let mut bytes = Vec::new();
        bytes.push(0u8); // q_type
        bytes.extend_from_slice(&1u64.to_be_bytes());
        bytes.extend_from_slice(&2u64.to_be_bytes());
        // Missing start_date and end_date

        let result = QueryDataMessage::deserialize(&bytes);
        assert!(result.is_err());
    }

    #[test]
    fn test_account_query_scenario() {
        // Scenario: Query for account (card_id = 0)
        let query = QueryDataMessage::new(0, 42, 0, 1000, 5000);
        let serialized = query.serialize();
        let deserialized = QueryDataMessage::deserialize(&serialized[1..]).unwrap();

        assert_eq!(deserialized.id, 42);
        assert_eq!(deserialized.card_id, 0);
    }

    #[test]
    fn test_historical_query_scenario() {
        // Scenario: Historical query (start_date = 0, end_date = 0)
        let query = QueryDataMessage::new(0, 100, 200, 0, 0);
        let serialized = query.serialize();
        let deserialized = QueryDataMessage::deserialize(&serialized[1..]).unwrap();

        assert_eq!(deserialized.start_date, 0);
        assert_eq!(deserialized.end_date, 0);
    }

    #[test]
    fn test_serialize_consistency() {
        let query = QueryDataMessage::new(0, 1, 2, 3, 4);
        let serialized1 = query.serialize();
        let serialized2 = query.serialize();

        assert_eq!(serialized1, serialized2);
    }

    #[test]
    fn test_deserialize_extra_bytes_ignored() {
        let mut bytes = Vec::new();
        bytes.push(0u8); // q_type
        bytes.extend_from_slice(&1u64.to_be_bytes());
        bytes.extend_from_slice(&2u64.to_be_bytes());
        bytes.extend_from_slice(&1000i64.to_be_bytes());
        bytes.extend_from_slice(&2000i64.to_be_bytes());
        bytes.extend_from_slice(&[99, 99, 99, 99]); // Extra bytes

        let result = QueryDataMessage::deserialize(&bytes);
        assert!(result.is_ok());

        let query = result.unwrap();
        assert_eq!(query.q_type, 0);
        assert_eq!(query.id, 1);
        assert_eq!(query.card_id, 2);
        assert_eq!(query.start_date, 1000);
        assert_eq!(query.end_date, 2000);
    }

    // NEW TESTS

    #[test]
    fn test_query_type_values() {
        // Test different query type values
        for q_type in [0, 1, 5, 10, 100, 255] {
            let query = QueryDataMessage::new(q_type, 1, 2, 3, 4);
            assert_eq!(query.query_type(), q_type);

            let serialized = query.serialize();
            let deserialized = QueryDataMessage::deserialize(&serialized[1..]).unwrap();
            assert_eq!(deserialized.query_type(), q_type);
        }
    }

    #[test]
    fn test_getters() {
        let query = QueryDataMessage::new(3, 100, 200, 1000, 2000);

        assert_eq!(query.query_type(), 3);
        assert_eq!(query.account_id(), 100);
        assert_eq!(query.card_id(), 200);
        assert_eq!(query.start_date(), 1000);
        assert_eq!(query.end_date(), 2000);
    }

    #[test]
    fn test_query_constant_in_serialized_output() {
        let query = QueryDataMessage::new(0, 1, 2, 3, 4);
        let serialized = query.serialize();

        // First byte should always be QUERY constant (message type identifier)
        // This byte is stripped by the receiver before calling deserialize()
        assert_eq!(serialized[0], QUERY);
        assert_eq!(serialized[0], 5);
    }

    #[test]
    fn test_date_range_validation_scenario() {
        // Test end date before start date (logically invalid but structurally valid)
        let query = QueryDataMessage::new(0, 1, 2, 5000, 1000);
        let serialized = query.serialize();
        let deserialized = QueryDataMessage::deserialize(&serialized[1..]).unwrap();

        assert_eq!(deserialized.start_date(), 5000);
        assert_eq!(deserialized.end_date(), 1000);
    }

    #[test]
    fn test_same_start_and_end_date() {
        let timestamp = 1704067200i64;
        let query = QueryDataMessage::new(0, 1, 2, timestamp, timestamp);
        let serialized = query.serialize();
        let deserialized = QueryDataMessage::deserialize(&serialized[1..]).unwrap();

        assert_eq!(deserialized.start_date(), timestamp);
        assert_eq!(deserialized.end_date(), timestamp);
    }

    #[test]
    fn test_specific_card_query_scenario() {
        // Scenario: Query for specific card (card_id != 0)
        let query = QueryDataMessage::new(1, 100, 555, 1000, 2000);
        let serialized = query.serialize();
        let deserialized = QueryDataMessage::deserialize(&serialized[1..]).unwrap();

        assert_eq!(deserialized.account_id(), 100);
        assert_eq!(deserialized.card_id(), 555);
        assert!(deserialized.card_id() != 0);
    }

    #[test]
    fn test_multiple_serialization_calls() {
        let query = QueryDataMessage::new(2, 999, 888, 7777, 6666);

        // Multiple serializations should produce identical results
        let s1 = query.serialize();
        let s2 = query.serialize();
        let s3 = query.serialize();

        assert_eq!(s1, s2);
        assert_eq!(s2, s3);
    }

    #[test]
    fn test_full_message_workflow() {
        // Simulates the complete workflow:
        // 1. Sender serializes the message
        // 2. Receiver reads the first byte (message type) to identify the message
        // 3. Receiver strips the first byte and deserializes the payload

        let original = QueryDataMessage::new(3, 12345, 67890, 1234567890, 1987654321);

        // Sender: serialize the message
        let serialized = original.serialize();

        // Receiver: read message type
        let message_type = serialized[0];
        assert_eq!(message_type, QUERY);

        // Receiver: strip message type and deserialize payload
        let payload = &serialized[1..];
        let deserialized = QueryDataMessage::deserialize(payload).unwrap();

        // Verify the roundtrip
        assert_eq!(original.q_type, deserialized.q_type);
        assert_eq!(original.id, deserialized.id);
        assert_eq!(original.card_id, deserialized.card_id);
        assert_eq!(original.start_date, deserialized.start_date);
        assert_eq!(original.end_date, deserialized.end_date);
    }
}
