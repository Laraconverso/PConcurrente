//! Archivo relacionado con la estructura de la información asociada a las transacciones.

use crate::utils::{read_f64, read_timestamp, read_u64, read_u8, CardId, GenId, PumpId, TimeStamp};
use std::io::Cursor;

pub const DATA_BYTES_SIZE: usize = 49;

/// Record de cada trasacción a enviar hacia el administrador.
///
/// Incluye:
/// - fecha de la transacción
/// - id de la estación de servicio donde se realizó la venta.
/// - surtidor que realizó la venta.
/// - tarjeta que llevo a cabo la venta.
/// - cantidad de combustible comprada (en litros).
/// - monto neto de la transacción.
/// - cargos extra de la transacción.
pub struct Data {
    date: TimeStamp,
    gas_station_id: GenId,
    pump_id: PumpId,
    card_id: CardId,
    gas_amount: f64,
    total: f64,
    fees: f64,
}

impl Data {
    pub fn new(
        date: TimeStamp,
        gas_station_id: GenId,
        pump_id: PumpId,
        card_id: CardId,
        gas_amount: f64,
        total: f64,
        fees: f64,
    ) -> Self {
        Self {
            date,
            gas_station_id,
            pump_id,
            card_id,
            gas_amount,
            total,
            fees,
        }
    }

    pub fn print_row(&self) {
        let width = 15;
        print!("│");
        print!(" {:>width$} │", self.date, width = width);
        print!(" {:>width$} │", self.gas_station_id, width = width);
        print!(" {:>width$} │", self.pump_id, width = width);
        print!(" {:>width$} │", self.card_id, width = width);
        print!(" {:>width$} │", self.gas_amount, width = width);
        print!(" {:>width$} │", self.total, width = width);
        print!(" {:>width$} │", self.fees, width = width);
        println!();
    }

    /// Serializa siguiendo el formato
    /// `[date_bytes, gas_station_id_bytes, pump_id_bytes, card_id_bytes, gas_amount_bytes, total_bytes, fees_bytes]`.
    /// Valores numéricos son serializados en big endian.
    pub fn serialize(&self) -> Vec<u8> {
        let mut buf = Vec::new();

        buf.extend_from_slice(&self.date.to_be_bytes());
        buf.extend_from_slice(&self.gas_station_id.to_be_bytes());
        buf.extend_from_slice(&self.pump_id.to_be_bytes());
        buf.extend_from_slice(&self.card_id.to_be_bytes());
        buf.extend_from_slice(&self.gas_amount.to_be_bytes());
        buf.extend_from_slice(&self.total.to_be_bytes());
        buf.extend_from_slice(&self.fees.to_be_bytes());

        buf
    }

    /// Deserializa una `Data` a partir de un conjunto de bytes.
    pub fn deserialize(mut bytes: &[u8]) -> Result<Self, String> {
        let mut cursor = Cursor::new(&mut bytes);

        let date = read_timestamp(&mut cursor)?;
        let gas_station_id = read_u64(&mut cursor)?;
        let pump_id = read_u8(&mut cursor)?;
        let card_id = read_u64(&mut cursor)?;
        let gas_amount = read_f64(&mut cursor)?;
        let total = read_f64(&mut cursor)?;
        let fees = read_f64(&mut cursor)?;

        Ok(Self {
            date,
            gas_station_id,
            pump_id,
            card_id,
            gas_amount,
            total,
            fees,
        })
    }

    pub fn print_header() {
        let headers = [
            "DATE",
            "GAS STATION ID",
            "PUMP",
            "CARD ID",
            "GAS AMOUNT",
            "TOTAL",
            "FEES",
        ];
        let width = 15;

        print!("┌");
        for (i, _) in headers.iter().enumerate() {
            print!("{:─<1$}", "", width + 2);
            if i == headers.len() - 1 {
                print!("┐");
            } else {
                print!("┬");
            }
        }
        println!();

        print!("│");
        for h in headers {
            print!(" {:>width$} │", h, width = width);
        }
        println!();

        print!("├");
        for (i, _) in headers.iter().enumerate() {
            print!("{:─<1$}", "", width + 2);
            if i == headers.len() - 1 {
                print!("┤");
            } else {
                print!("┼");
            }
        }
        println!();
    }

    pub fn print_footer() {
        let width = 15;
        let columns = 7;

        // Bottom border
        print!("└");
        for i in 0..columns {
            print!("{:─<1$}", "", width + 2);
            if i == columns - 1 {
                print!("┘");
            } else {
                print!("┴");
            }
        }
        println!();
    }
}

// TODO: REVISAR
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_creates_data() {
        let data = Data::new(
            1234567890, // date
            100,        // gas_station_id
            5,          // pump_id
            999,        // card_id
            45.5,       // gas_amount
            2500.75,    // total
            125.50,     // fees
        );

        assert_eq!(data.date, 1234567890);
        assert_eq!(data.gas_station_id, 100);
        assert_eq!(data.pump_id, 5);
        assert_eq!(data.card_id, 999);
        assert_eq!(data.gas_amount, 45.5);
        assert_eq!(data.total, 2500.75);
        assert_eq!(data.fees, 125.50);
    }

    #[test]
    fn test_serialize_produces_correct_size() {
        let data = Data::new(1000, 1, 1, 100, 50.0, 3000.0, 150.0);
        let serialized = data.serialize();

        // i64(8) + u64(8) + u8(1) + u64(8) + f64(8) + f64(8) + f64(8) = 49 bytes
        assert_eq!(serialized.len(), DATA_BYTES_SIZE);
        assert_eq!(serialized.len(), 49);
    }

    #[test]
    fn test_serialize_byte_order() {
        let data = Data::new(1000, 100, 5, 200, 45.5, 2500.0, 125.0);
        let serialized = data.serialize();

        // Verify big-endian encoding for first field (date - i64)
        let date_bytes = &serialized[0..8];
        assert_eq!(i64::from_be_bytes(date_bytes.try_into().unwrap()), 1000);

        // Verify gas_station_id (u64)
        let station_bytes = &serialized[8..16];
        assert_eq!(u64::from_be_bytes(station_bytes.try_into().unwrap()), 100);

        // Verify pump_id (u8)
        assert_eq!(serialized[16], 5);

        // Verify card_id (u64)
        let card_bytes = &serialized[17..25];
        assert_eq!(u64::from_be_bytes(card_bytes.try_into().unwrap()), 200);
    }

    #[test]
    fn test_deserialize_from_valid_bytes() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&1000i64.to_be_bytes());
        bytes.extend_from_slice(&100u64.to_be_bytes());
        bytes.extend_from_slice(&5u8.to_be_bytes());
        bytes.extend_from_slice(&200u64.to_be_bytes());
        bytes.extend_from_slice(&45.5f64.to_be_bytes());
        bytes.extend_from_slice(&2500.0f64.to_be_bytes());
        bytes.extend_from_slice(&125.0f64.to_be_bytes());

        let result = Data::deserialize(&bytes);
        assert!(result.is_ok());

        let data = result.unwrap();
        assert_eq!(data.date, 1000);
        assert_eq!(data.gas_station_id, 100);
        assert_eq!(data.pump_id, 5);
        assert_eq!(data.card_id, 200);
        assert_eq!(data.gas_amount, 45.5);
        assert_eq!(data.total, 2500.0);
        assert_eq!(data.fees, 125.0);
    }

    #[test]
    fn test_serialize_deserialize_roundtrip() {
        let original = Data::new(1234567890, 999, 42, 12345, 65.75, 4250.50, 212.53);

        let serialized = original.serialize();
        let deserialized = Data::deserialize(&serialized).unwrap();

        assert_eq!(original.date, deserialized.date);
        assert_eq!(original.gas_station_id, deserialized.gas_station_id);
        assert_eq!(original.pump_id, deserialized.pump_id);
        assert_eq!(original.card_id, deserialized.card_id);
        assert_eq!(original.gas_amount, deserialized.gas_amount);
        assert_eq!(original.total, deserialized.total);
        assert_eq!(original.fees, deserialized.fees);
    }

    #[test]
    fn test_with_zero_values() {
        let data = Data::new(0, 0, 0, 0, 0.0, 0.0, 0.0);
        let serialized = data.serialize();
        let deserialized = Data::deserialize(&serialized).unwrap();

        assert_eq!(deserialized.date, 0);
        assert_eq!(deserialized.gas_station_id, 0);
        assert_eq!(deserialized.pump_id, 0);
        assert_eq!(deserialized.card_id, 0);
        assert_eq!(deserialized.gas_amount, 0.0);
        assert_eq!(deserialized.total, 0.0);
        assert_eq!(deserialized.fees, 0.0);
    }

    #[test]
    fn test_with_negative_timestamp() {
        // Date before Unix epoch
        let data = Data::new(-1000, 100, 5, 200, 45.5, 2500.0, 125.0);
        let serialized = data.serialize();
        let deserialized = Data::deserialize(&serialized).unwrap();

        assert_eq!(deserialized.date, -1000);
    }

    #[test]
    fn test_with_min_max_timestamp() {
        // Minimum i64 timestamp
        let data_min = Data::new(i64::MIN, 1, 1, 1, 1.0, 1.0, 1.0);
        let serialized_min = data_min.serialize();
        let deserialized_min = Data::deserialize(&serialized_min).unwrap();
        assert_eq!(deserialized_min.date, i64::MIN);

        // Maximum i64 timestamp
        let data_max = Data::new(i64::MAX, 1, 1, 1, 1.0, 1.0, 1.0);
        let serialized_max = data_max.serialize();
        let deserialized_max = Data::deserialize(&serialized_max).unwrap();
        assert_eq!(deserialized_max.date, i64::MAX);
    }

    #[test]
    fn test_with_max_u64_values() {
        let data = Data::new(1000, u64::MAX, 1, u64::MAX, 50.0, 3000.0, 150.0);
        let serialized = data.serialize();
        let deserialized = Data::deserialize(&serialized).unwrap();

        assert_eq!(deserialized.gas_station_id, u64::MAX);
        assert_eq!(deserialized.card_id, u64::MAX);
    }

    #[test]
    fn test_with_max_pump_id() {
        let data = Data::new(1000, 100, u8::MAX, 200, 45.5, 2500.0, 125.0);
        let serialized = data.serialize();
        let deserialized = Data::deserialize(&serialized).unwrap();

        assert_eq!(deserialized.pump_id, u8::MAX);
        assert_eq!(deserialized.pump_id, 255);
    }

    #[test]
    fn test_deserialize_with_insufficient_bytes() {
        let bytes = vec![0u8; 30]; // Only 30 bytes instead of 49
        let result = Data::deserialize(&bytes);

        assert!(result.is_err());
    }

    #[test]
    fn test_deserialize_with_empty_bytes() {
        let bytes: Vec<u8> = vec![];
        let result = Data::deserialize(&bytes);

        assert!(result.is_err());
    }

    #[test]
    fn test_deserialize_with_partial_data() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&1000i64.to_be_bytes());
        bytes.extend_from_slice(&100u64.to_be_bytes());
        // Missing remaining fields

        let result = Data::deserialize(&bytes);
        assert!(result.is_err());
    }

    #[test]
    fn test_realistic_gas_transaction() {
        // Realistic gas station transaction
        let data = Data::new(
            1730419200, // Nov 1, 2024 00:00:00 UTC
            101,        // Gas station ID
            3,          // Pump 3
            67890,      // Card ID
            45.5,       // 45.5 liters
            2750.25,    // Total amount
            137.51,     // Fees (5% of total)
        );

        let serialized = data.serialize();
        let deserialized = Data::deserialize(&serialized).unwrap();

        assert_eq!(deserialized.date, 1730419200);
        assert_eq!(deserialized.gas_station_id, 101);
        assert_eq!(deserialized.pump_id, 3);
        assert_eq!(deserialized.card_id, 67890);
        assert_eq!(deserialized.gas_amount, 45.5);
        assert_eq!(deserialized.total, 2750.25);
        assert_eq!(deserialized.fees, 137.51);
    }

    #[test]
    fn test_multiple_pump_ids() {
        // Test various pump IDs (1-255)
        for pump_id in [1u8, 5, 10, 50, 100, 200, 255] {
            let data = Data::new(1000, 100, pump_id, 200, 45.5, 2500.0, 125.0);
            let serialized = data.serialize();
            let deserialized = Data::deserialize(&serialized).unwrap();

            assert_eq!(deserialized.pump_id, pump_id);
        }
    }

    #[test]
    fn test_float_precision() {
        let data = Data::new(
            1000,
            100,
            5,
            200,
            45.123456789,
            2500.987654321,
            125.111222333,
        );

        let serialized = data.serialize();
        let deserialized = Data::deserialize(&serialized).unwrap();

        // f64 should maintain precision through serialization
        assert_eq!(deserialized.gas_amount, 45.123456789);
        assert_eq!(deserialized.total, 2500.987654321);
        assert_eq!(deserialized.fees, 125.111222333);
    }

    #[test]
    fn test_negative_float_values() {
        // Could represent refunds or corrections
        let data = Data::new(1000, 100, 5, 200, -10.0, -500.0, -25.0);
        let serialized = data.serialize();
        let deserialized = Data::deserialize(&serialized).unwrap();

        assert_eq!(deserialized.gas_amount, -10.0);
        assert_eq!(deserialized.total, -500.0);
        assert_eq!(deserialized.fees, -25.0);
    }

    #[test]
    fn test_special_float_values() {
        let data = Data::new(1000, 100, 5, 200, f64::INFINITY, f64::NEG_INFINITY, 0.0);

        let serialized = data.serialize();
        let deserialized = Data::deserialize(&serialized).unwrap();

        assert_eq!(deserialized.gas_amount, f64::INFINITY);
        assert_eq!(deserialized.total, f64::NEG_INFINITY);
    }

    #[test]
    fn test_serialize_consistency() {
        let data = Data::new(1000, 100, 5, 200, 45.5, 2500.0, 125.0);

        let serialized1 = data.serialize();
        let serialized2 = data.serialize();

        assert_eq!(serialized1, serialized2);
    }

    #[test]
    fn test_large_gas_amounts() {
        // Very large transaction (e.g., fleet refueling)
        let data = Data::new(
            1000, 100, 5, 200, 99999.99,   // Large gas amount
            9999999.99, // Large total
            500000.00,  // Large fees
        );

        let serialized = data.serialize();
        let deserialized = Data::deserialize(&serialized).unwrap();

        assert_eq!(deserialized.gas_amount, 99999.99);
        assert_eq!(deserialized.total, 9999999.99);
        assert_eq!(deserialized.fees, 500000.00);
    }

    #[test]
    fn test_small_transaction() {
        // Small transaction (e.g., motorcycle or small purchase)
        let data = Data::new(
            1000, 100, 5, 200, 5.25,   // Small gas amount
            315.75, // Small total
            15.79,  // Small fees
        );

        let serialized = data.serialize();
        let deserialized = Data::deserialize(&serialized).unwrap();

        assert_eq!(deserialized.gas_amount, 5.25);
        assert_eq!(deserialized.total, 315.75);
        assert_eq!(deserialized.fees, 15.79);
    }

    #[test]
    fn test_deserialize_extra_bytes_ignored() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&1000i64.to_be_bytes());
        bytes.extend_from_slice(&100u64.to_be_bytes());
        bytes.extend_from_slice(&5u8.to_be_bytes());
        bytes.extend_from_slice(&200u64.to_be_bytes());
        bytes.extend_from_slice(&45.5f64.to_be_bytes());
        bytes.extend_from_slice(&2500.0f64.to_be_bytes());
        bytes.extend_from_slice(&125.0f64.to_be_bytes());
        bytes.extend_from_slice(&[99, 99, 99]); // Extra bytes

        let result = Data::deserialize(&bytes);
        assert!(result.is_ok());

        let data = result.unwrap();
        assert_eq!(data.date, 1000);
        assert_eq!(data.gas_station_id, 100);
    }

    #[test]
    fn test_realistic_date_range() {
        // Test with realistic Unix timestamps
        let dates = [
            1704067200i64, // Jan 1, 2024
            1730419200i64, // Nov 1, 2024
            1735689600i64, // Jan 1, 2025
        ];

        for date in dates {
            let data = Data::new(date, 100, 5, 200, 45.5, 2500.0, 125.0);
            let serialized = data.serialize();
            let deserialized = Data::deserialize(&serialized).unwrap();

            assert_eq!(deserialized.date, date);
        }
    }

    #[test]
    fn test_transaction_with_no_fees() {
        let data = Data::new(1000, 100, 5, 200, 45.5, 2500.0, 0.0);
        let serialized = data.serialize();
        let deserialized = Data::deserialize(&serialized).unwrap();

        assert_eq!(deserialized.fees, 0.0);
    }

    #[test]
    fn test_data_bytes_size_constant_matches() {
        let data = Data::new(1, 1, 1, 1, 1.0, 1.0, 1.0);
        let serialized = data.serialize();

        assert_eq!(serialized.len(), DATA_BYTES_SIZE);
        assert_eq!(DATA_BYTES_SIZE, 49);
    }
}
