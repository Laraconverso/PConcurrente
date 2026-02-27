use crate::utils::{read_f64, read_timestamp, read_u64, read_u8, CardId, TimeStamp};
use chrono::{DateTime, TimeZone, Utc};
use serde::{Deserialize, Serialize};
use std::io::Cursor;

#[derive(Clone, Serialize, Deserialize)]
pub struct Transaction {
    date: TimeStamp,
    card_id: CardId,
    gas_station_id: u64,
    pump_number: u8,
    amount: f64,
    fees: f64,
}

impl Transaction {
    pub fn new(
        date: TimeStamp,
        card_id: CardId,
        gas_station_id: u64,
        pump_number: u8,
        amount: f64,
        fees: f64,
    ) -> Self {
        Self {
            date,
            card_id,
            gas_station_id,
            pump_number,
            amount,
            fees,
        }
    }

    pub fn date(&self) -> TimeStamp {
        self.date
    }

    pub fn card_id(&self) -> CardId {
        self.card_id
    }

    pub fn gas_station_id(&self) -> u64 {
        self.gas_station_id
    }

    pub fn pump_number(&self) -> u8 {
        self.pump_number
    }

    pub fn amount(&self) -> f64 {
        self.amount
    }

    pub fn fees(&self) -> f64 {
        self.fees
    }

    pub fn serialize(&self) -> Vec<u8> {
        let mut buf = Vec::new();

        buf.extend_from_slice(&self.date.to_be_bytes());
        buf.extend_from_slice(&self.card_id.to_be_bytes());
        buf.extend_from_slice(&self.gas_station_id.to_be_bytes());
        buf.push(self.pump_number);
        buf.extend_from_slice(&self.amount.to_be_bytes());
        buf.extend_from_slice(&self.fees.to_be_bytes());

        buf
    }

    pub fn deserialize(mut bytes: &[u8]) -> Result<Self, String> {
        let mut cursor = Cursor::new(&mut bytes);

        let date = read_timestamp(&mut cursor)?; // 8
        let card_id = read_u64(&mut cursor)?; // 16
        let gas_station_id = read_u64(&mut cursor)?; // 24
        let pump_number = read_u8(&mut cursor)?; // 25
        let amount = read_f64(&mut cursor)?; // 33
        let fees = read_f64(&mut cursor)?; // 41

        Ok(Self {
            date,
            card_id,
            gas_station_id,
            pump_number,
            amount,
            fees,
        })
    }

    pub fn print_row(&self) {
        let width = 22;
        print!("│");
        print!(
            " {:>width$} │",
            map_timestamp_to_string(self.date),
            width = width
        );
        print!(" {:>width$} │", self.gas_station_id, width = width);
        print!(" {:>width$} │", self.pump_number, width = width);
        print!(" {:>width$} │", self.card_id, width = width);
        print!(" {:>width$} │", self.amount, width = width);
        print!(" {:>width$} │", self.fees, width = width);
        println!();
    }

    pub fn print_header() {
        let headers = [
            "DATE",
            "GAS STATION ID",
            "PUMP",
            "CARD ID",
            "GAS AMOUNT",
            "FEES",
        ];
        let width = 22;

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

    pub fn print_footer() -> Result<(), &'static str> {
        let width = 22;
        let columns = 6;

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
        Ok(())
    }
}

fn map_timestamp_to_string(timestamp: TimeStamp) -> String {
    let datetime_utc: DateTime<Utc> =
        Utc.timestamp_opt(timestamp, 0).latest().unwrap_or_else(|| {
            panic!("Error, wrong timestamp: {}", timestamp);
        });

    datetime_utc.format("%d-%m-%Y %H:%M:%S").to_string()
}

/// Devuelve verdadero si la fecha indicada se encuentra dentro de los límites dados.
pub fn transaction_belongs(
    lower_bound: TimeStamp,
    upper_bound: TimeStamp,
    date: TimeStamp,
) -> bool {
    date >= lower_bound && date <= upper_bound
}

pub fn serialize_transactions(transactions: Vec<Transaction>) -> Vec<u8> {
    let mut buf = Vec::new();

    buf.push(0);
    // Itero N veces para sacar N transacciones
    buf.extend_from_slice(&(transactions.len() as u64).to_be_bytes());

    for transaction in transactions {
        buf.extend_from_slice(&transaction.serialize())
    }

    buf
}

pub fn deserialize_transactions(mut bytes: &[u8]) -> Result<Vec<Transaction>, String> {
    let mut transactions = Vec::new();

    let n = read_u64(&mut bytes)?;

    for _ in 0..n {
        let (txn_bytes, rest) = bytes.split_at(41);
        bytes = rest;

        let transaction = Transaction::deserialize(txn_bytes)?;
        transactions.push(transaction);
    }

    Ok(transactions)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transaction_new() {
        let txn = Transaction::new(1234567890, 9876543210, 111, 5, 50.75, 2.50);

        assert_eq!(txn.date(), 1234567890);
        assert_eq!(txn.card_id(), 9876543210);
        assert_eq!(txn.gas_station_id(), 111);
        assert_eq!(txn.pump_number(), 5);
        assert_eq!(txn.amount(), 50.75);
        assert_eq!(txn.fees(), 2.50);
    }

    #[test]
    fn test_transaction_serialize_deserialize() {
        let original = Transaction::new(1609459200, 1122334455, 999, 3, 75.50, 3.25);

        let serialized = original.serialize();
        let deserialized = Transaction::deserialize(&serialized).unwrap();

        assert_eq!(deserialized.date(), original.date());
        assert_eq!(deserialized.card_id(), original.card_id());
        assert_eq!(deserialized.gas_station_id(), original.gas_station_id());
        assert_eq!(deserialized.pump_number(), original.pump_number());
        assert_eq!(deserialized.amount(), original.amount());
        assert_eq!(deserialized.fees(), original.fees());
    }

    #[test]
    fn test_transaction_serialize_length() {
        let txn = Transaction::new(1234567890, 9876543210, 111, 5, 50.75, 2.50);
        let serialized = txn.serialize();

        // Expected: 8 (timestamp) + 8 (card_id) + 8 (gas_station_id) + 1 (pump_number) + 8 (amount) + 8 (fees) = 41 bytes
        assert_eq!(serialized.len(), 41);
    }

    #[test]
    fn test_transaction_deserialize_invalid_data() {
        let invalid_data = vec![0u8; 10]; // Too short
        let result = Transaction::deserialize(&invalid_data);

        assert!(result.is_err());
    }

    #[test]
    fn test_transaction_belongs_within_bounds() {
        let lower = 1000;
        let upper = 2000;
        let date = 1500;

        assert!(transaction_belongs(lower, upper, date));
    }

    #[test]
    fn test_transaction_belongs_at_lower_bound() {
        let lower = 1000;
        let upper = 2000;
        let date = 1000;

        assert!(transaction_belongs(lower, upper, date));
    }

    #[test]
    fn test_transaction_belongs_at_upper_bound() {
        let lower = 1000;
        let upper = 2000;
        let date = 2000;

        assert!(transaction_belongs(lower, upper, date));
    }

    #[test]
    fn test_transaction_belongs_below_lower_bound() {
        let lower = 1000;
        let upper = 2000;
        let date = 999;

        assert!(!transaction_belongs(lower, upper, date));
    }

    #[test]
    fn test_transaction_belongs_above_upper_bound() {
        let lower = 1000;
        let upper = 2000;
        let date = 2001;

        assert!(!transaction_belongs(lower, upper, date));
    }

    #[test]
    fn test_serialize_transactions_empty() {
        let transactions = Vec::new();
        let serialized = serialize_transactions(transactions);

        // Expected: 1 byte (type) + 8 bytes (count = 0) = 9 bytes
        assert_eq!(serialized.len(), 9);
        assert_eq!(serialized[0], 0);
    }

    #[test]
    fn test_serialize_transactions_single() {
        let txn = Transaction::new(1234567890, 9876543210, 111, 5, 50.75, 2.50);
        let transactions = vec![txn];
        let serialized = serialize_transactions(transactions);

        // Expected: 1 byte (type) + 8 bytes (count = 1) + 41 bytes (transaction) = 50 bytes
        assert_eq!(serialized.len(), 50);
    }

    #[test]
    fn test_serialize_transactions_multiple() {
        let txn1 = Transaction::new(1234567890, 1111111111, 100, 1, 30.00, 1.50);
        let txn2 = Transaction::new(1234567900, 2222222222, 200, 2, 40.00, 2.00);
        let txn3 = Transaction::new(1234567910, 3333333333, 300, 3, 50.00, 2.50);
        let transactions = vec![txn1, txn2, txn3];
        let serialized = serialize_transactions(transactions);

        // Expected: 1 byte (type) + 8 bytes (count = 3) + 41*3 bytes (transactions) = 132 bytes
        assert_eq!(serialized.len(), 132);
    }

    #[test]
    fn test_deserialize_transactions_empty() {
        let mut data = vec![0u8]; // Type byte
        data.extend_from_slice(&0u64.to_be_bytes()); // Count = 0

        let result = deserialize_transactions(&data[1..]).unwrap();
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_deserialize_transactions_single() {
        let txn = Transaction::new(1234567890, 9876543210, 111, 5, 50.75, 2.50);
        let transactions = vec![txn];
        let serialized = serialize_transactions(transactions);

        let deserialized = deserialize_transactions(&serialized[1..]).unwrap();
        assert_eq!(deserialized.len(), 1);
        assert_eq!(deserialized[0].date(), 1234567890);
        assert_eq!(deserialized[0].card_id(), 9876543210);
    }

    #[test]
    fn test_deserialize_transactions_multiple() {
        let txn1 = Transaction::new(1234567890, 1111111111, 100, 1, 30.00, 1.50);
        let txn2 = Transaction::new(1234567900, 2222222222, 200, 2, 40.00, 2.00);
        let txn3 = Transaction::new(1234567910, 3333333333, 300, 3, 50.00, 2.50);
        let transactions = vec![txn1, txn2, txn3];
        let serialized = serialize_transactions(transactions);

        let deserialized = deserialize_transactions(&serialized[1..]).unwrap();
        assert_eq!(deserialized.len(), 3);
        assert_eq!(deserialized[0].gas_station_id(), 100);
        assert_eq!(deserialized[1].gas_station_id(), 200);
        assert_eq!(deserialized[2].gas_station_id(), 300);
    }

    #[test]
    fn test_serialize_deserialize_transactions_roundtrip() {
        let txn1 = Transaction::new(1609459200, 1111111111, 100, 1, 30.00, 1.50);
        let txn2 = Transaction::new(1609459300, 2222222222, 200, 2, 40.00, 2.00);
        let original_transactions = vec![txn1, txn2];

        let serialized = serialize_transactions(original_transactions.clone());
        let deserialized = deserialize_transactions(&serialized[1..]).unwrap();

        assert_eq!(deserialized.len(), original_transactions.len());
        for (original, restored) in original_transactions.iter().zip(deserialized.iter()) {
            assert_eq!(restored.date(), original.date());
            assert_eq!(restored.card_id(), original.card_id());
            assert_eq!(restored.gas_station_id(), original.gas_station_id());
            assert_eq!(restored.pump_number(), original.pump_number());
            assert_eq!(restored.amount(), original.amount());
            assert_eq!(restored.fees(), original.fees());
        }
    }

    #[test]
    fn test_transaction_with_zero_values() {
        let txn = Transaction::new(0, 0, 0, 0, 0.0, 0.0);
        let serialized = txn.serialize();
        let deserialized = Transaction::deserialize(&serialized).unwrap();

        assert_eq!(deserialized.date(), 0);
        assert_eq!(deserialized.card_id(), 0);
        assert_eq!(deserialized.gas_station_id(), 0);
        assert_eq!(deserialized.pump_number(), 0);
        assert_eq!(deserialized.amount(), 0.0);
        assert_eq!(deserialized.fees(), 0.0);
    }

    #[test]
    fn test_transaction_with_max_values() {
        let txn = Transaction::new(i64::MAX, u64::MAX, u64::MAX, u8::MAX, f64::MAX, f64::MAX);
        let serialized = txn.serialize();
        let deserialized = Transaction::deserialize(&serialized).unwrap();

        assert_eq!(deserialized.date(), i64::MAX);
        assert_eq!(deserialized.card_id(), u64::MAX);
        assert_eq!(deserialized.gas_station_id(), u64::MAX);
        assert_eq!(deserialized.pump_number(), u8::MAX);
        assert_eq!(deserialized.amount(), f64::MAX);
        assert_eq!(deserialized.fees(), f64::MAX);
    }

    #[test]
    fn test_transaction_with_negative_floats() {
        let txn = Transaction::new(1234567890, 9876543210, 111, 5, -50.75, -2.50);
        let serialized = txn.serialize();
        let deserialized = Transaction::deserialize(&serialized).unwrap();

        assert_eq!(deserialized.amount(), -50.75);
        assert_eq!(deserialized.fees(), -2.50);
    }
}
