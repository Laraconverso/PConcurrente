//! Archivo con la declaración de la estructura de mensaje `RegisterAccountMessage`
//! Usada para dar aviso al SV central del alta de una nueva cuenta.

use crate::utils::{read_string, read_u64};
use std::io::Cursor;

pub const REG_ACCOUNT: u8 = 0;

pub const DEFAULT_LIMIT: f64 = 100000.0;
pub const REG_CODE: u8 = 0;

#[derive(Debug, Clone, PartialEq)]
/// Mensaje que transporta el username (para login) y display name (para mostrar).
/// La cuenta se crea SIN regiones habilitadas inicialmente.
pub struct RegisterAccountMessage {
    username: String,     // Username único e inmutable (para login)
    display_name: String, // Nombre para mostrar (puede tener espacios)
}

impl RegisterAccountMessage {
    pub fn new(username: String, display_name: String) -> Self {
        Self {
            username,
            display_name,
        }
    }

    pub fn username(&self) -> String {
        self.username.clone()
    }

    pub fn name(&self) -> String {
        self.display_name.clone()
    }

    pub fn display_name(&self) -> String {
        self.display_name.clone()
    }

    /// Serializa siguiendo el formato `[code, username_len, username, display_name_len, display_name]`.
    /// Valores numéricos son serializados en big endian.
    pub fn serialize(&self) -> Vec<u8> {
        let mut buf = Vec::new();

        buf.push(REG_ACCOUNT);

        // Serializar username
        let username_bytes = self.username.as_bytes();
        let username_len = username_bytes.len() as u64;
        buf.extend_from_slice(&username_len.to_be_bytes());
        buf.extend_from_slice(username_bytes);

        // Serializar display_name
        let display_bytes = self.display_name.as_bytes();
        let display_len = display_bytes.len() as u64;
        buf.extend_from_slice(&display_len.to_be_bytes());
        buf.extend_from_slice(display_bytes);

        buf
    }

    /// Deserializa un `RegisterAccountMessage` a partir de un conjunto de bytes.
    pub fn deserialize(mut bytes: &[u8]) -> Result<Self, String> {
        let mut cursor = Cursor::new(&mut bytes);

        // Deserializar username
        let username_len = read_u64(&mut cursor)? as usize;
        let username = read_string(&mut cursor, username_len)?;

        // Deserializar display_name
        let display_len = read_u64(&mut cursor)? as usize;
        let display_name = read_string(&mut cursor, display_len)?;

        Ok(Self {
            username,
            display_name,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Basic Constructor Tests
    #[test]
    fn test_new_basic() {
        let msg = RegisterAccountMessage::new("john_doe".to_string(), "John Doe".to_string());
        assert_eq!(msg.username(), "john_doe");
        assert_eq!(msg.display_name(), "John Doe");
    }

    #[test]
    fn test_new_with_empty_strings() {
        let msg = RegisterAccountMessage::new("".to_string(), "".to_string());
        assert_eq!(msg.username(), "");
        assert_eq!(msg.display_name(), "");
    }

    #[test]
    fn test_new_with_special_characters() {
        let msg =
            RegisterAccountMessage::new("user_123!@#".to_string(), "Display Name 🚀".to_string());
        assert_eq!(msg.username(), "user_123!@#");
        assert_eq!(msg.display_name(), "Display Name 🚀");
    }

    #[test]
    fn test_name_returns_display_name() {
        let msg = RegisterAccountMessage::new("username".to_string(), "Display Name".to_string());
        assert_eq!(msg.name(), msg.display_name());
    }

    // Serialization Tests
    #[test]
    fn test_serialize_basic() {
        let msg = RegisterAccountMessage::new("testuser".to_string(), "Test User".to_string());
        let bytes = msg.serialize();

        // Expected format:
        // - 1 byte: REG_ACCOUNT code
        // - 8 bytes: username length (8 as u64 big endian)
        // - 8 bytes: "testuser"
        // - 8 bytes: display_name length (9 as u64 big endian)
        // - 9 bytes: "Test User"
        assert_eq!(bytes.len(), 1 + 8 + 8 + 8 + 9);

        // Check code
        assert_eq!(bytes[0], REG_ACCOUNT);

        // Check username length
        let username_len = u64::from_be_bytes([
            bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7], bytes[8],
        ]);
        assert_eq!(username_len, 8);

        // Check username
        let username = String::from_utf8(bytes[9..17].to_vec()).unwrap();
        assert_eq!(username, "testuser");

        // Check display_name length
        let display_len = u64::from_be_bytes([
            bytes[17], bytes[18], bytes[19], bytes[20], bytes[21], bytes[22], bytes[23], bytes[24],
        ]);
        assert_eq!(display_len, 9);

        // Check display_name
        let display_name = String::from_utf8(bytes[25..34].to_vec()).unwrap();
        assert_eq!(display_name, "Test User");
    }

    #[test]
    fn test_serialize_empty_fields() {
        let msg = RegisterAccountMessage::new("".to_string(), "".to_string());
        let bytes = msg.serialize();

        // 1 byte code + 8 bytes username_len + 0 bytes username + 8 bytes display_len + 0 bytes display
        assert_eq!(bytes.len(), 1 + 8 + 0 + 8 + 0);

        assert_eq!(bytes[0], REG_ACCOUNT);

        let username_len = u64::from_be_bytes([
            bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7], bytes[8],
        ]);
        assert_eq!(username_len, 0);

        let display_len = u64::from_be_bytes([
            bytes[9], bytes[10], bytes[11], bytes[12], bytes[13], bytes[14], bytes[15], bytes[16],
        ]);
        assert_eq!(display_len, 0);
    }

    #[test]
    fn test_serialize_unicode() {
        let msg =
            RegisterAccountMessage::new("user_日本".to_string(), "Usuario Español 🇪🇸".to_string());
        let bytes = msg.serialize();

        let username_bytes = "user_日本".as_bytes();
        let display_bytes = "Usuario Español 🇪🇸".as_bytes();

        assert_eq!(bytes[0], REG_ACCOUNT);

        let username_len = u64::from_be_bytes([
            bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7], bytes[8],
        ]);
        assert_eq!(username_len as usize, username_bytes.len());

        assert_eq!(
            bytes.len(),
            1 + 8 + username_bytes.len() + 8 + display_bytes.len()
        );
    }

    #[test]
    fn test_serialize_long_strings() {
        let long_username = "a".repeat(5000);
        let long_display = "b".repeat(7000);
        let msg = RegisterAccountMessage::new(long_username.clone(), long_display.clone());
        let bytes = msg.serialize();

        assert_eq!(bytes.len(), 1 + 8 + 5000 + 8 + 7000);
        assert_eq!(bytes[0], REG_ACCOUNT);
    }

    // Deserialization Tests
    #[test]
    fn test_deserialize_basic() {
        let msg = RegisterAccountMessage::new("myuser".to_string(), "My Name".to_string());
        let bytes = msg.serialize();

        let deserialized = RegisterAccountMessage::deserialize(&bytes[1..]).unwrap(); // Skip REG_ACCOUNT code

        assert_eq!(deserialized.username(), "myuser");
        assert_eq!(deserialized.display_name(), "My Name");
    }

    #[test]
    fn test_deserialize_empty_fields() {
        let msg = RegisterAccountMessage::new("".to_string(), "".to_string());
        let bytes = msg.serialize();

        let deserialized = RegisterAccountMessage::deserialize(&bytes[1..]).unwrap();

        assert_eq!(deserialized.username(), "");
        assert_eq!(deserialized.display_name(), "");
    }

    #[test]
    fn test_serialize_deserialize_roundtrip() {
        let original = RegisterAccountMessage::new(
            "roundtrip_user".to_string(),
            "Round Trip Name".to_string(),
        );
        let bytes = original.serialize();
        let deserialized = RegisterAccountMessage::deserialize(&bytes[1..]).unwrap();

        assert_eq!(deserialized.username(), original.username());
        assert_eq!(deserialized.display_name(), original.display_name());
    }

    #[test]
    fn test_serialize_deserialize_unicode_roundtrip() {
        let original =
            RegisterAccountMessage::new("user_中文".to_string(), "Café España 日本 🏢".to_string());
        let bytes = original.serialize();
        let deserialized = RegisterAccountMessage::deserialize(&bytes[1..]).unwrap();

        assert_eq!(deserialized.username(), "user_中文");
        assert_eq!(deserialized.display_name(), "Café España 日本 🏢");
    }

    #[test]
    fn test_serialize_deserialize_long_strings_roundtrip() {
        let long_username = "x".repeat(10000);
        let long_display = "y".repeat(15000);
        let original = RegisterAccountMessage::new(long_username.clone(), long_display.clone());
        let bytes = original.serialize();
        let deserialized = RegisterAccountMessage::deserialize(&bytes[1..]).unwrap();

        assert_eq!(deserialized.username(), long_username);
        assert_eq!(deserialized.display_name(), long_display);
    }

    // Error Handling Tests
    #[test]
    fn test_deserialize_insufficient_bytes() {
        let bytes = vec![0, 0, 0, 0]; // Only 4 bytes, not enough for u64 length
        let result = RegisterAccountMessage::deserialize(&bytes);

        assert!(result.is_err());
    }

    #[test]
    fn test_deserialize_truncated_username() {
        let mut bytes = Vec::new();
        // Say we have 10 bytes of username, but only provide 5
        bytes.extend_from_slice(&10u64.to_be_bytes());
        bytes.extend_from_slice(b"short");

        let result = RegisterAccountMessage::deserialize(&bytes);
        assert!(result.is_err());
    }

    #[test]
    fn test_deserialize_missing_display_name_length() {
        let mut bytes = Vec::new();
        let username = "test";
        bytes.extend_from_slice(&(username.len() as u64).to_be_bytes());
        bytes.extend_from_slice(username.as_bytes());
        // Missing the 8 bytes for display_name length

        let result = RegisterAccountMessage::deserialize(&bytes);
        assert!(result.is_err());
    }

    #[test]
    fn test_deserialize_truncated_display_name() {
        let mut bytes = Vec::new();
        let username = "test";
        bytes.extend_from_slice(&(username.len() as u64).to_be_bytes());
        bytes.extend_from_slice(username.as_bytes());
        bytes.extend_from_slice(&20u64.to_be_bytes()); // Say 20 bytes
        bytes.extend_from_slice(b"short"); // But only provide 5

        let result = RegisterAccountMessage::deserialize(&bytes);
        assert!(result.is_err());
    }

    #[test]
    fn test_deserialize_empty_input() {
        let bytes = vec![];
        let result = RegisterAccountMessage::deserialize(&bytes);
        assert!(result.is_err());
    }

    // Edge Cases
    #[test]
    fn test_username_with_spaces() {
        let msg = RegisterAccountMessage::new(
            "user name with spaces".to_string(),
            "Display Name".to_string(),
        );
        assert_eq!(msg.username(), "user name with spaces");
    }

    #[test]
    fn test_display_name_with_newlines() {
        let msg = RegisterAccountMessage::new(
            "username".to_string(),
            "Line 1\nLine 2\nLine 3".to_string(),
        );
        let bytes = msg.serialize();
        let deserialized = RegisterAccountMessage::deserialize(&bytes[1..]).unwrap();
        assert_eq!(deserialized.display_name(), "Line 1\nLine 2\nLine 3");
    }

    #[test]
    fn test_same_username_and_display_name() {
        let msg = RegisterAccountMessage::new("same".to_string(), "same".to_string());
        assert_eq!(msg.username(), msg.display_name());
    }

    #[test]
    fn test_max_u64_length_value() {
        // Test with actual data, not with u64::MAX length
        let username = "u".repeat(1000);
        let display = "d".repeat(2000);
        let msg = RegisterAccountMessage::new(username.clone(), display.clone());
        let bytes = msg.serialize();
        let deserialized = RegisterAccountMessage::deserialize(&bytes[1..]).unwrap();

        assert_eq!(deserialized.username(), username);
        assert_eq!(deserialized.display_name(), display);
    }

    #[test]
    fn test_binary_safety() {
        // Test that we can handle non-UTF8 valid strings (though Rust strings are UTF-8)
        let username = "user\x00null".to_string();
        let display = "display\x00name".to_string();
        let msg = RegisterAccountMessage::new(username.clone(), display.clone());
        let bytes = msg.serialize();
        let deserialized = RegisterAccountMessage::deserialize(&bytes[1..]).unwrap();

        assert_eq!(deserialized.username(), username);
        assert_eq!(deserialized.display_name(), display);
    }

    #[test]
    fn test_reg_account_constant() {
        let msg = RegisterAccountMessage::new("test".to_string(), "test".to_string());
        let bytes = msg.serialize();
        assert_eq!(bytes[0], REG_ACCOUNT);
        assert_eq!(bytes[0], 0);
    }

    #[test]
    fn test_reg_code_constant() {
        assert_eq!(REG_CODE, 0);
        assert_eq!(REG_CODE, REG_ACCOUNT);
    }

    #[test]
    fn test_clone_works() {
        let msg = RegisterAccountMessage::new("user".to_string(), "display".to_string());
        let cloned = msg.clone();
        assert_eq!(msg.username(), cloned.username());
        assert_eq!(msg.display_name(), cloned.display_name());
    }

    #[test]
    fn test_debug_format() {
        let msg = RegisterAccountMessage::new("testuser".to_string(), "Test User".to_string());
        let debug_str = format!("{:?}", msg);
        assert!(debug_str.contains("testuser"));
        assert!(debug_str.contains("Test User"));
    }
}
