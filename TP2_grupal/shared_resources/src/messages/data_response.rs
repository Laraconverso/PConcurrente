//! Archivo con la estructura que representa los mensajes de retorno desde SV central
//! hacia el administrador.

use crate::database::transaction::{deserialize_transactions, Transaction};
use crate::messages::{
    account::register_account::REG_ACCOUNT,
    card::register_card::REG_CARD,
    query::query_data::QUERY,
    remove_regions::UPD_REMOVE_REGIONS,
    update_data::{UPD_ACCOUNT_LIMIT, UPD_ACCOUNT_NAME, UPD_CARD},
    update_regions::UPD_REGIONS,
};
use crate::utils::{frame_payload, read_f64, read_string, read_u64, read_u8, GenId, SUCCESS};
use std::fmt;
use std::fmt::Formatter;
use std::io::Cursor;

const TOTAL_WIDTH: usize = 150;

#[derive(Debug)]
/// Mensaje de confirmación indicándo si la operación fue un éxito (0) o no (1).
pub struct DataResponseMessage {
    code: u8,
    status: u8,
    id: GenId,
    piggyback: Option<Vec<u8>>,
}

impl fmt::Display for DataResponseMessage {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let status = match self.status {
            0 => "OK",
            _ => "Error",
        };
        write!(f, "{} {}", status, self.id)
    }
}

impl DataResponseMessage {
    pub fn new(code: u8, status: u8, id: GenId, piggyback: Option<Vec<u8>>) -> Self {
        Self {
            code,
            status,
            id,
            piggyback,
        }
    }

    pub fn status(&self) -> u8 {
        self.status
    }

    pub fn code(&self) -> u8 {
        self.code
    }

    pub fn piggyback(&self) -> Option<Vec<u8>> {
        self.piggyback.clone()
    }

    pub fn display_response(&self) {
        match self.code {
            REG_ACCOUNT => display_status_response(
                self,
                format!(
                    "Account successfully registered with id: \"{}\"\nLogging in...",
                    self.id
                ),
                "registering an account".to_string(),
            ),
            REG_CARD => display_status_response(
                self,
                format!("Card successfully added with id: \"{}\"", self.id),
                "registering a card".to_string(),
            ),
            UPD_ACCOUNT_LIMIT => display_status_response(
                self,
                format!("Account limit \"{}\" successfully modified", self.id),
                "updating account limits".to_string(),
            ),
            UPD_ACCOUNT_NAME => display_status_response(
                self,
                format!("Account name \"{}\" successfully updated", self.id),
                "updating account name".to_string(),
            ),
            UPD_CARD => display_status_response(
                self,
                format!("Card limit \"{}\" successfully modified", self.id),
                "updating card limits".to_string(),
            ),
            UPD_REGIONS => display_status_response(
                self,
                format!(
                    "Regiones actualizadas correctamente para cuenta {}",
                    self.id
                ),
                "updating regions".to_string(),
            ),
            UPD_REMOVE_REGIONS => display_status_response(
                self,
                format!("Regiones removidas correctamente de cuenta {}", self.id),
                "removing regions".to_string(),
            ),
            QUERY => {
                if let Err(e) = display_query_response(self) {
                    println!("Error while displaying response: {e}");
                }
            }
            _ => println!(
                "Something went wrong, unknown code \"{}\" received",
                self.code
            ),
        }
    }

    /// Serializa el mensaje para poder enviarlo por un socket como un vector de bytes
    /// `[code_bytes, status_bytes, id_bytes]`
    pub fn serialize(&self) -> Vec<u8> {
        let mut buf = Vec::new();

        buf.push(self.code);
        buf.push(self.status);
        buf.extend_from_slice(&self.id.to_be_bytes());

        match self.piggyback.clone() {
            Some(piggyback) => {
                buf.extend_from_slice(&frame_payload(Vec::from(&*piggyback)));
            }
            None => {
                buf.extend_from_slice(&0u64.to_be_bytes());
            }
        }

        buf
    }

    /// Deserializa un `DataResponseMessage` a partir de un vector de bytes.
    pub fn deserialize(mut bytes: &[u8]) -> Result<Self, String> {
        let mut cursor = Cursor::new(&mut bytes);

        let code = read_u8(&mut cursor)?;
        let status = read_u8(&mut cursor)?;
        let id = read_u64(&mut cursor)?;

        let piggyback_len = read_u64(&mut cursor)?;
        if piggyback_len > 0 {
            let start = cursor.position() as usize;
            let end = start + piggyback_len as usize;
            let piggyback = bytes[start..end].to_vec();
            return Ok(Self {
                code,
                status,
                id,
                piggyback: Some(piggyback),
            });
        }

        Ok(Self {
            code,
            status,
            id,
            piggyback: None,
        })
    }

    pub fn id(&self) -> GenId {
        self.id
    }
}

fn display_status_response(msg: &DataResponseMessage, success_msg: String, fail_msg: String) {
    if msg.status() != SUCCESS {
        // Si hay piggyback con error detallado, mostrarlo
        if let Some(piggyback) = &msg.piggyback {
            if let Ok(error_detail) = String::from_utf8(piggyback.clone()) {
                println!("Error: {}", error_detail);
            } else {
                println!("Error while {}", fail_msg);
            }
        } else {
            println!("Error while {}", fail_msg);
        }
    } else {
        println!("{}", success_msg);
    }
}

fn display_query_response(msg: &DataResponseMessage) -> Result<(), String> {
    if msg.status() != SUCCESS {
        return Err("Error while retrieving data".into());
    }
    if msg.piggyback().is_none() {
        println!("There's no data");
        return Ok(());
    }
    let mut bytes = msg.piggyback().unwrap();
    let mut cursor = Cursor::new(&mut bytes);
    let q_type = read_u8(&mut cursor)?;

    if q_type == 0 {
        let transactions = deserialize_transactions(&bytes[1..])?;
        Transaction::print_header();
        for transaction in transactions {
            transaction.print_row();
        }
        let _ = Transaction::print_footer();
        return Ok(());
    }
    // 1
    let ancho: usize = TOTAL_WIDTH;

    println!("\n{:^ancho$}", " INFORMACIÓN DE CUENTA ");

    // Leer username
    let username_len = read_u64(&mut cursor)?;
    let username = read_string(&mut cursor, username_len as usize)?;

    // Leer display_name
    let display_len = read_u64(&mut cursor)?;
    let display_name = read_string(&mut cursor, display_len as usize)?;

    // Leer regiones habilitadas
    let num_regions = read_u64(&mut cursor)?;
    let mut regions: Vec<(String, f64)> = Vec::new();
    for _ in 0..num_regions {
        let region_len = read_u64(&mut cursor)?;
        let region_name = read_string(&mut cursor, region_len as usize)?;
        let region_limit = read_f64(&mut cursor)?;
        regions.push((region_name, region_limit));
    }

    let total_spent = read_f64(&mut cursor)?;

    print_info_line(
        "USERNAME".to_string(),
        format!("@{}", username),
        TOTAL_WIDTH,
    );
    print_info_line("DISPLAY NAME".to_string(), display_name, TOTAL_WIDTH);
    print_info_line(
        "ACCOUNT ID".to_string(),
        format!("{}", msg.id()),
        TOTAL_WIDTH,
    );

    // Mostrar regiones habilitadas
    if regions.is_empty() {
        print_info_line(
            "REGIONES".to_string(),
            "Ninguna habilitada".to_string(),
            TOTAL_WIDTH,
        );
    } else {
        println!();
        println!("{:^ancho$}", " REGIONES HABILITADAS ");
        for (region_name, region_limit) in &regions {
            let available = region_limit - total_spent;
            let usage = if *region_limit > 0.0 {
                (total_spent / region_limit) * 100.0
            } else {
                0.0
            };
            println!("\nRegión: {}", region_name);
            print_info_line(
                "  LIMIT".to_string(),
                format!("${:.2}", region_limit),
                TOTAL_WIDTH,
            );
            print_info_line(
                "  SPENT".to_string(),
                format!("${:.2}", total_spent),
                TOTAL_WIDTH,
            );
            print_info_line(
                "  AVAILABLE".to_string(),
                format!("${:.2}", available),
                TOTAL_WIDTH,
            );
            print_info_line("  USAGE".to_string(), format!("{:.1}%", usage), TOTAL_WIDTH);
        }
    }

    println!();
    println!("\n{:^ancho$}", " TARJETAS ");
    let cards = read_u64(&mut cursor)?;
    for _ in 0..cards {
        let c_id = read_u64(&mut cursor)?;
        let limit = read_f64(&mut cursor)?;
        let spent = read_f64(&mut cursor)?;
        let available = limit - spent;
        let usage = if limit > 0.0 {
            (spent / limit) * 100.0
        } else {
            0.0
        };

        println!("Card #{}", c_id);
        print_info_line("LIMIT".to_string(), format!("${:.2}", limit), TOTAL_WIDTH);
        print_info_line("SPENT".to_string(), format!("${:.2}", spent), TOTAL_WIDTH);
        print_info_line(
            "AVAILABLE".to_string(),
            format!("${:.2}", available),
            TOTAL_WIDTH,
        );
        print_info_line("USAGE".to_string(), format!("{:.1}%\n", usage), TOTAL_WIDTH);
    }
    println!();
    Ok(())
}

fn print_info_line(label: String, value: String, total_width: usize) {
    let left = format!("{}: ", label);
    let dots_count = total_width - left.len() - value.len();
    let dots = ".".repeat(dots_count);
    println!("{}{}{}", left, dots, value);
}

#[cfg(test)]
mod tests {
    use super::*;

    // Helper function to get all code variants for testing
    // Mapped from original: RegAccMsg=0, UpdAccMsg=1, etc.
    fn get_all_operation_codes() -> Vec<u8> {
        vec![0, 1, 2, 3, 4, 5, 6]
    }

    #[test]
    fn test_new_creates_data_response_message() {
        // OperationCode::RegAccMsg -> 0
        let msg = DataResponseMessage::new(0, 0, 12345, None);

        assert_eq!(msg.code, 0);
        assert_eq!(msg.status, 0);
        assert_eq!(msg.id, 12345);
    }

    #[test]
    fn test_new_with_success_status() {
        let msg = DataResponseMessage::new(0, 0, 999, None);

        assert_eq!(msg.status, 0);
    }

    #[test]
    fn test_new_with_error_status() {
        // OperationCode::Error -> 6
        let msg = DataResponseMessage::new(6, 1, 999, None);

        assert_eq!(msg.status, 1);
    }

    #[test]
    fn test_serialize_produces_correct_size() {
        let msg = DataResponseMessage::new(0, 0, 12345, None);

        let serialized = msg.serialize();

        // u8(code) + u8(status) + u64(id) + u64(piggyback_len)
        // 1 + 1 + 8 + 8 = 18 bytes
        assert_eq!(serialized.len(), 18);
    }

    #[test]
    fn test_serialize_byte_structure() {
        let msg = DataResponseMessage::new(0, 0, 12345, None);

        let serialized = msg.serialize();

        // First byte should be operation code (0)
        assert_eq!(serialized[0], 0);

        // Second byte should be status
        assert_eq!(serialized[1], 0);

        // Bytes 2..10 should be id in big-endian
        let id_bytes = &serialized[2..10];
        assert_eq!(u64::from_be_bytes(id_bytes.try_into().unwrap()), 12345);

        // Bytes 10..18 should be piggyback length (0)
        let piggy_len = &serialized[10..18];
        assert_eq!(u64::from_be_bytes(piggy_len.try_into().unwrap()), 0);
    }

    #[test]
    fn test_deserialize_from_valid_bytes() {
        let mut bytes = Vec::new();
        bytes.push(0u8); // RegAccMsg
        bytes.push(0u8); // Success status
        bytes.extend_from_slice(&12345u64.to_be_bytes());
        bytes.extend_from_slice(&0u64.to_be_bytes()); // Piggyback len

        let result = DataResponseMessage::deserialize(&bytes);
        assert!(result.is_ok());

        let msg = result.unwrap();
        assert_eq!(msg.code, 0);
        assert_eq!(msg.status, 0);
        assert_eq!(msg.id, 12345);
    }

    #[test]
    fn test_serialize_deserialize_roundtrip() {
        // OperationCode::UpdAccMsg -> 1
        let original = DataResponseMessage::new(1, 0, 999999, None);

        let serialized = original.serialize();
        let deserialized = DataResponseMessage::deserialize(&serialized).unwrap();

        assert_eq!(original.code, deserialized.code);
        assert_eq!(original.status, deserialized.status);
        assert_eq!(original.id, deserialized.id);
    }

    #[test]
    fn test_all_operation_codes_roundtrip() {
        for code in get_all_operation_codes() {
            let original = DataResponseMessage::new(code, 0, 100, None);
            let serialized = original.serialize();
            let deserialized = DataResponseMessage::deserialize(&serialized).unwrap();

            assert_eq!(original.code, deserialized.code);
        }
    }

    #[test]
    fn test_with_max_status_value() {
        let msg = DataResponseMessage::new(0, u8::MAX, 12345, None);

        let serialized = msg.serialize();
        let deserialized = DataResponseMessage::deserialize(&serialized).unwrap();

        assert_eq!(deserialized.status, u8::MAX);
        assert_eq!(deserialized.status, 255);
    }

    #[test]
    fn test_with_max_id_value() {
        let msg = DataResponseMessage::new(0, 0, u64::MAX, None);

        let serialized = msg.serialize();
        let deserialized = DataResponseMessage::deserialize(&serialized).unwrap();

        assert_eq!(deserialized.id, u64::MAX);
    }

    #[test]
    fn test_with_zero_id() {
        let msg = DataResponseMessage::new(0, 0, 0, None);

        let serialized = msg.serialize();
        let deserialized = DataResponseMessage::deserialize(&serialized).unwrap();

        assert_eq!(deserialized.id, 0);
    }

    #[test]
    fn test_deserialize_with_insufficient_bytes() {
        let bytes = vec![0u8; 10]; // Only 10 bytes instead of 18
        let result = DataResponseMessage::deserialize(&bytes);

        assert!(result.is_err());
    }

    #[test]
    fn test_deserialize_with_empty_bytes() {
        let bytes: Vec<u8> = vec![];
        let result = DataResponseMessage::deserialize(&bytes);

        assert!(result.is_err());
    }

    #[test]
    fn test_deserialize_with_partial_data() {
        let mut bytes = Vec::new();
        bytes.push(0u8);
        bytes.push(0u8);
        // Missing id bytes and piggyback len

        let result = DataResponseMessage::deserialize(&bytes);
        assert!(result.is_err());
    }

    #[test]
    fn test_deserialize_extra_bytes_ignored() {
        let mut bytes = Vec::new();
        bytes.push(0u8);
        bytes.push(0u8);
        bytes.extend_from_slice(&12345u64.to_be_bytes());
        bytes.extend_from_slice(&0u64.to_be_bytes()); // Piggyback len
        bytes.extend_from_slice(&[99, 99, 99]); // Extra bytes

        let result = DataResponseMessage::deserialize(&bytes);
        assert!(result.is_ok());

        let msg = result.unwrap();
        assert_eq!(msg.id, 12345);
    }

    #[test]
    fn test_serialize_consistency() {
        let msg = DataResponseMessage::new(0, 0, 12345, None);

        let serialized1 = msg.serialize();
        let serialized2 = msg.serialize();

        assert_eq!(serialized1, serialized2);
    }

    #[test]
    fn test_register_account_success() {
        let msg = DataResponseMessage::new(
            0, // RegAccMsg
            0, // Success
            12345, None,
        );

        let serialized = msg.serialize();
        let deserialized = DataResponseMessage::deserialize(&serialized).unwrap();

        assert_eq!(deserialized.code, 0);
        assert_eq!(deserialized.status, 0);
    }

    #[test]
    fn test_register_account_failure() {
        let msg = DataResponseMessage::new(
            0, // RegAccMsg
            1, // Failure
            12345, None,
        );

        let serialized = msg.serialize();
        let deserialized = DataResponseMessage::deserialize(&serialized).unwrap();

        assert_eq!(deserialized.code, 0);
        assert_eq!(deserialized.status, 1);
    }

    #[test]
    fn test_update_account_success() {
        let msg = DataResponseMessage::new(1, 0, 67890, None); // UpdAccMsg

        let serialized = msg.serialize();
        let deserialized = DataResponseMessage::deserialize(&serialized).unwrap();

        assert_eq!(deserialized.code, 1);
        assert_eq!(deserialized.status, 0);
    }

    #[test]
    fn test_delete_account_success() {
        let msg = DataResponseMessage::new(2, 0, 11111, None); // DelAccMsg

        let serialized = msg.serialize();
        let deserialized = DataResponseMessage::deserialize(&serialized).unwrap();

        assert_eq!(deserialized.code, 2);
        assert_eq!(deserialized.status, 0);
    }

    #[test]
    fn test_register_card_success() {
        let msg = DataResponseMessage::new(3, 0, 22222, None); // RegCardMsg

        let serialized = msg.serialize();
        let deserialized = DataResponseMessage::deserialize(&serialized).unwrap();

        assert_eq!(deserialized.code, 3);
        assert_eq!(deserialized.status, 0);
    }

    #[test]
    fn test_update_card_success() {
        let msg = DataResponseMessage::new(4, 0, 33333, None); // UpdCardMsg

        let serialized = msg.serialize();
        let deserialized = DataResponseMessage::deserialize(&serialized).unwrap();

        assert_eq!(deserialized.code, 4);
        assert_eq!(deserialized.status, 0);
    }

    #[test]
    fn test_delete_card_success() {
        let msg = DataResponseMessage::new(5, 0, 44444, None); // DelCardMsg

        let serialized = msg.serialize();
        let deserialized = DataResponseMessage::deserialize(&serialized).unwrap();

        assert_eq!(deserialized.code, 5);
        assert_eq!(deserialized.status, 0);
    }

    #[test]
    fn test_error_operation() {
        let msg = DataResponseMessage::new(6, 1, 55555, None); // Error

        let serialized = msg.serialize();
        let deserialized = DataResponseMessage::deserialize(&serialized).unwrap();

        assert_eq!(deserialized.code, 6);
        assert_eq!(deserialized.status, 1);
    }

    #[test]
    fn test_different_status_codes() {
        for status in [0u8, 1, 2, 10, 50, 100, 200, 255] {
            let msg = DataResponseMessage::new(0, status, 100, None);

            let serialized = msg.serialize();
            let deserialized = DataResponseMessage::deserialize(&serialized).unwrap();

            assert_eq!(deserialized.status, status);
        }
    }

    #[test]
    fn test_different_account_ids() {
        let ids = [0u64, 1, 100, 999, 12345, 999999, u64::MAX];

        for id in ids {
            let msg = DataResponseMessage::new(0, 0, id, None);

            let serialized = msg.serialize();
            let deserialized = DataResponseMessage::deserialize(&serialized).unwrap();

            assert_eq!(deserialized.id, id);
        }
    }

    #[test]
    fn test_all_operations_with_different_ids() {
        let operations = get_all_operation_codes();

        for (idx, op) in operations.iter().enumerate() {
            let msg = DataResponseMessage::new(*op, 0, (idx as u64 + 1) * 1000, None);

            let serialized = msg.serialize();
            let deserialized = DataResponseMessage::deserialize(&serialized).unwrap();

            assert_eq!(deserialized.code, *op);
            assert_eq!(deserialized.id, (idx as u64 + 1) * 1000);
        }
    }

    #[test]
    fn test_minimal_valid_message() {
        let msg = DataResponseMessage::new(0, 0, 0, None);

        let serialized = msg.serialize();
        assert_eq!(serialized.len(), 18); // 1 + 1 + 8 + 8

        let deserialized = DataResponseMessage::deserialize(&serialized).unwrap();
        assert_eq!(deserialized.status, 0);
        assert_eq!(deserialized.id, 0);
    }

    #[test]
    fn test_maximal_valid_message() {
        let msg = DataResponseMessage::new(6, u8::MAX, u64::MAX, None);

        let serialized = msg.serialize();
        assert_eq!(serialized.len(), 18);

        let deserialized = DataResponseMessage::deserialize(&serialized).unwrap();
        assert_eq!(deserialized.status, u8::MAX);
        assert_eq!(deserialized.id, u64::MAX);
    }

    #[test]
    fn test_byte_order_big_endian() {
        let msg = DataResponseMessage::new(0, 0, 0x0102030405060708, None);

        let serialized = msg.serialize();

        // Verify big-endian encoding of id
        let id_bytes = &serialized[2..10];
        assert_eq!(id_bytes[0], 0x01);
        assert_eq!(id_bytes[1], 0x02);
        assert_eq!(id_bytes[2], 0x03);
        assert_eq!(id_bytes[3], 0x04);
        assert_eq!(id_bytes[4], 0x05);
        assert_eq!(id_bytes[5], 0x06);
        assert_eq!(id_bytes[6], 0x07);
        assert_eq!(id_bytes[7], 0x08);
    }

    #[test]
    fn test_deserialize_with_invalid_operation_code() {
        // Should deserialize as is (u8), since structure does not enforce Enum
        let mut bytes = Vec::new();
        bytes.push(99u8); // Invalid operation code
        bytes.push(0u8);
        bytes.extend_from_slice(&12345u64.to_be_bytes());
        bytes.extend_from_slice(&0u64.to_be_bytes()); // Piggyback len

        let result = DataResponseMessage::deserialize(&bytes);
        assert!(result.is_ok());

        let msg = result.unwrap();
        assert_eq!(msg.code, 99);
    }
}
