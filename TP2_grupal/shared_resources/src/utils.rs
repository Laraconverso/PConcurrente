//! Lista de funciones y estructuras comunes y auxiliares.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::{SystemTime, UNIX_EPOCH};

pub type TimeStamp = i64;
pub type AccountId = u64;
pub type CardId = u64;
pub type GenId = u64; // TODO: Es el genérico que podría dar problemas en DataResponseMessage si no se banca usar un u64 cualquiera...
pub type PumpId = u8;

pub const SUCCESS: u8 = 0;
pub const FAILURE: u8 = 1;

pub trait ByteVec {
    fn extend_from_string(&mut self, name: &str);
}

impl ByteVec for Vec<u8> {
    fn extend_from_string(&mut self, name: &str) {
        let bytes = name.as_bytes();
        self.extend_from_slice(&(bytes.len() as u64).to_be_bytes());
        self.extend_from_slice(bytes);
    }
}

/// Función auxiliar para leer un `u8` desde un stream de bytes.
pub fn read_u8<R: Read>(reader: &mut R) -> Result<u8, &'static str> {
    let mut buf = [0u8; 1];
    reader
        .read_exact(&mut buf)
        .map_err(|_| "error when reading u8")?;
    Ok(u8::from_be_bytes(buf))
}

/// Función auxiliar para leer un `u16` desde un stream de bytes.
pub fn read_u16<R: Read>(reader: &mut R) -> Result<u16, &'static str> {
    let mut buf = [0u8; 2];
    reader
        .read_exact(&mut buf)
        .map_err(|_| "error when reading u8")?;
    Ok(u16::from_be_bytes(buf))
}

/// Función auxiliar para leer un `u64` desde un stream de bytes.
pub fn read_u64<R: Read>(reader: &mut R) -> Result<u64, &'static str> {
    let mut buf = [0u8; 8];
    reader
        .read_exact(&mut buf)
        .map_err(|_| "error when reading u8")?;
    Ok(u64::from_be_bytes(buf))
}

/// Función auxiliar para leer un `f64` desde un stream de bytes.
pub fn read_f64<R: Read>(reader: &mut R) -> Result<f64, &'static str> {
    let mut buf = [0u8; 8];
    reader
        .read_exact(&mut buf)
        .map_err(|_| "error when reading u8")?;
    Ok(f64::from_be_bytes(buf))
}

/// Función auxiliar para leer un `TimeStamp` desde un stream de bytes.
pub fn read_timestamp<R: Read>(reader: &mut R) -> Result<TimeStamp, &'static str> {
    let mut buf = [0u8; 8];
    reader
        .read_exact(&mut buf)
        .map_err(|_| "error when reading u8")?;
    Ok(i64::from_be_bytes(buf))
}

/// Función auxiliar para leer un `String` desde un stream de bytes.
pub fn read_string<R: Read>(reader: &mut R, len: usize) -> Result<String, &'static str> {
    let mut buf = vec![0u8; len];
    reader
        .read_exact(&mut buf)
        .map_err(|_| "error when reading a string")?;
    Ok(String::from_utf8(buf).unwrap())
}

pub fn frame_payload(mut bytes: Vec<u8>) -> Vec<u8> {
    let len = bytes.len() as u64;

    let len_bytes = len.to_be_bytes();

    let mut framed = Vec::with_capacity(8 + bytes.len());
    framed.extend_from_slice(&len_bytes);
    framed.append(&mut bytes);
    framed
}

pub fn write_and_flush(stream: &mut TcpStream, bytes: Vec<u8>) -> Result<(), String> {
    if stream.write_all(&bytes).is_err() {
        println!("Error while sending data");
        return Err("Error stream".parse().unwrap());
    }
    if stream.flush().is_err() {
        println!("Error while flushing stream");
        return Err("Error flushing".parse().unwrap());
    }
    Ok(())
}

/// Obtiene timestamp en formato HH:MM:SS.mmm
pub fn get_timestamp() -> String {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
    let secs = now.as_secs();
    let millis = now.subsec_millis();
    let hours = (secs / 3600) % 24;
    let minutes = (secs / 60) % 60;
    let seconds = secs % 60;
    format!("{:02}:{:02}:{:02}.{:03}", hours, minutes, seconds, millis)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    // --- Tests de Constantes ---
    #[test]
    fn test_constants() {
        assert_eq!(SUCCESS, 0);
        assert_eq!(FAILURE, 1);
    }

    // --- Tests de Trait ByteVec ---
    #[test]
    fn test_byte_vec_extend_from_string() {
        let mut buffer: Vec<u8> = Vec::new();
        let text = "Hola".to_string();

        buffer.extend_from_string(&text);

        // Estructura esperada:
        // 8 bytes de longitud (4 en big endian) + bytes del string
        // 0, 0, 0, 0, 0, 0, 0, 4, 'H', 'o', 'l', 'a'

        assert_eq!(buffer.len(), 12); // 8 + 4

        // Verificar longitud escrita
        let len_bytes = &buffer[0..8];
        assert_eq!(u64::from_be_bytes(len_bytes.try_into().unwrap()), 4);

        // Verificar contenido
        assert_eq!(&buffer[8..], "Hola".as_bytes());
    }

    // --- Tests de Lectura (Read Functions) ---

    #[test]
    fn test_read_u8_success() {
        let data = vec![255];
        let mut cursor = Cursor::new(data);

        let result = read_u8(&mut cursor);
        assert_eq!(result.unwrap(), 255);
    }

    #[test]
    fn test_read_u8_error_eof() {
        let data = vec![]; // Vacío
        let mut cursor = Cursor::new(data);

        let result = read_u8(&mut cursor);
        assert_eq!(result, Err("error when reading u8"));
    }

    #[test]
    fn test_read_u16_success() {
        // 513 en Big Endian = [0x02, 0x01] -> (2 * 256) + 1
        let data = vec![2, 1];
        let mut cursor = Cursor::new(data);

        let result = read_u16(&mut cursor);
        assert_eq!(result.unwrap(), 513);
    }

    #[test]
    fn test_read_u64_success() {
        let val: u64 = 1234567890;
        let data = val.to_be_bytes().to_vec();
        let mut cursor = Cursor::new(data);

        let result = read_u64(&mut cursor);
        assert_eq!(result.unwrap(), val);
    }

    #[test]
    fn test_read_f64_success() {
        let val: f64 = 123.456;
        let data = val.to_be_bytes().to_vec();
        let mut cursor = Cursor::new(data);

        let result = read_f64(&mut cursor);
        // Comparamos float por igualdad exacta porque viene de bytes exactos
        assert_eq!(result.unwrap(), val);
    }

    #[test]
    fn test_read_timestamp_success() {
        let val: TimeStamp = 1600000000;
        let data = val.to_be_bytes().to_vec();
        let mut cursor = Cursor::new(data);

        let result = read_timestamp(&mut cursor);
        assert_eq!(result.unwrap(), val);
    }

    #[test]
    fn test_read_string_success() {
        let text = "Rust";
        let data = text.as_bytes().to_vec();
        let mut cursor = Cursor::new(data);

        // Debemos pasar la longitud exacta
        let result = read_string(&mut cursor, 4);
        assert_eq!(result.unwrap(), "Rust");
    }

    #[test]
    fn test_read_string_error_insufficient_bytes() {
        let text = "Hi";
        let data = text.as_bytes().to_vec();
        let mut cursor = Cursor::new(data);

        // Pedimos 10 bytes pero solo hay 2
        let result = read_string(&mut cursor, 10);
        assert_eq!(result, Err("error when reading a string"));
    }

    #[test]
    #[should_panic]
    fn test_read_string_panic_invalid_utf8() {
        // El código original usa unwrap() en String::from_utf8,
        // por lo que esperamos un panic si los bytes no son UTF-8 válido.
        let invalid_utf8 = vec![0xFF, 0xFF];
        let mut cursor = Cursor::new(invalid_utf8);

        let _ = read_string(&mut cursor, 2);
    }

    // --- Test de Frame Payload ---

    #[test]
    fn test_frame_payload() {
        let payload = vec![10, 20, 30]; // 3 bytes

        let framed = frame_payload(payload.clone());

        // Esperamos: 8 bytes de longitud + 3 bytes de payload = 11 bytes
        assert_eq!(framed.len(), 11);

        // Verificar cabecera de longitud (8 bytes big endian)
        let len_header = &framed[0..8];
        assert_eq!(u64::from_be_bytes(len_header.try_into().unwrap()), 3);

        // Verificar payload original
        assert_eq!(&framed[8..], &payload[..]);
    }

    #[test]
    fn test_frame_payload_empty() {
        let payload = vec![];
        let framed = frame_payload(payload);

        assert_eq!(framed.len(), 8);
        assert_eq!(u64::from_be_bytes(framed[0..8].try_into().unwrap()), 0);
    }

    // --- Test de Timestamp Formatting ---

    #[test]
    fn test_get_timestamp_format() {
        let ts = get_timestamp();

        // El formato es HH:MM:SS.mmm (ej: "14:30:01.123")
        // Longitud total: 2 + 1 + 2 + 1 + 2 + 1 + 3 = 12 caracteres
        assert_eq!(ts.len(), 12);

        // Verificar que contiene los separadores
        assert_eq!(ts.chars().nth(2), Some(':'));
        assert_eq!(ts.chars().nth(5), Some(':'));
        assert_eq!(ts.chars().nth(8), Some('.'));

        // Verificar que son dígitos
        let parts: Vec<&str> = ts.split(|c| c == ':' || c == '.').collect();
        assert_eq!(parts.len(), 4); // HH, MM, SS, mmm

        assert!(parts[0].parse::<u32>().is_ok()); // Horas
        assert!(parts[1].parse::<u32>().is_ok()); // Minutos
        assert!(parts[2].parse::<u32>().is_ok()); // Segundos
        assert!(parts[3].parse::<u32>().is_ok()); // Milisegundos
    }
}
