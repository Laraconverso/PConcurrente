use serde::{Deserialize, Serialize};
use shared_resources::messages::protocol::TransactionResponse;
use shared_resources::messages::transaction::TransactionMessage;
use shared_resources::utils::get_timestamp;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

/// Log informativo con timestamp y componente
fn log_info(port: u16, msg: &str) {
    println!("[{}] [PUMP #{}] [INFO] {}", get_timestamp(), port, msg);
}

/// Log de error con timestamp y componente
fn log_error(port: u16, msg: &str) {
    eprintln!("[{}] [PUMP #{}] [ERROR] {}", get_timestamp(), port, msg);
}

pub struct Pump {
    response_listener_port: u16,
    local_address: String,
}

impl Pump {
    ///Crea una nueva instancia de un surtidor con el puerto de escucha y la dirección local especificados.
    pub fn new(response_listener_port: u16, local_address: String) -> Self {
        Self {
            response_listener_port,
            local_address,
        }
    }

    /// Inicia el listener para recibir respuestas de la Gas Station
    pub fn start_response_listener(&self) -> std::thread::JoinHandle<()> {
        let port = self.response_listener_port;

        thread::spawn(move || {
            let address = format!("0.0.0.0:{}", port);
            let listener = match TcpListener::bind(&address) {
                Ok(l) => {
                    log_info(port, &format!("Escuchando respuestas en {}", address));
                    l
                }
                Err(e) => {
                    log_error(
                        port,
                        &format!("Error al bindear puerto de respuestas: {}", e),
                    );
                    return;
                }
            };

            for stream in listener.incoming() {
                match stream {
                    Ok(stream) => {
                        thread::spawn(move || {
                            if let Err(e) = Self::handle_response(stream, port) {
                                log_error(port, &format!("Error procesando respuesta: {}", e));
                            }
                        });
                    }
                    Err(e) => {
                        log_error(
                            port,
                            &format!("Error aceptando conexión de respuesta: {}", e),
                        );
                    }
                }
            }
        })
    }

    /// Maneja una respuesta recibida desde la estación de servicio.
    fn handle_response(stream: TcpStream, port: u16) -> std::io::Result<()> {
        let mut reader = BufReader::new(stream);
        let mut response_str = String::new();
        reader.read_line(&mut response_str)?;

        let response: TransactionResponse = serde_json::from_str(&response_str)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?;

        match response.status {
            shared_resources::messages::protocol::TransactionStatus::Accepted => {
                log_info(
                    port,
                    &format!("TX {}: {}", response.transaction_id, response.message),
                );
            }
            _ => {
                log_error(
                    port,
                    &format!("TX {}: {}", response.transaction_id, response.message),
                );
            }
        }

        Ok(())
    }

    /// Genera un mensaje de transacción con los datos proporcionados.
    pub fn generate_transaction(
        &mut self,
        account_id: u64,
        card_id: u64,
        amount: f64,
        station_id: u64,
    ) -> TransactionMessage {
        TransactionMessage {
            transaction_id: Uuid::new_v4(),
            account_id,
            card_id,
            amount,
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            station_id,
            pump_address: Some(self.local_address.clone()),
        }
    }

    /// Envía un mensaje de transacción a la estación de servicio.
    pub fn send_transaction(
        &self,
        transaction: &TransactionMessage,
        station_address: &str,
    ) -> std::io::Result<()> {
        // Connect to the station address
        let mut stream = TcpStream::connect(station_address)?;
        let serialized =
            serde_json::to_vec(transaction).map_err(|e| std::io::Error::other(e.to_string()))?;
        stream.write_all(&serialized)?;
        log_info(
            self.response_listener_port,
            &format!("Transacción enviada: {}", transaction.transaction_id),
        );
        Ok(())
    }

    /// Inicia una API TCP para recibir requests de transacciones en formato JSON
    pub fn start_transaction_api(
        api_port: u16,
        station_addr: String,
        response_port: u16,
        local_addr: String,
    ) {
        thread::spawn(move || {
            let listener = match TcpListener::bind(format!("0.0.0.0:{}", api_port)) {
                Ok(l) => {
                    log_info(
                        response_port,
                        &format!("API TCP escuchando en puerto {}", api_port),
                    );
                    log_info(
                        response_port,
                        "   Formato: {\"account_id\": 123, \"card_id\": 456, \"amount\": 100.0}",
                    );
                    log_info(response_port, &format!("   Ejemplo: echo '{{\"account_id\": 1, \"card_id\": 5001, \"amount\": 500.0}}' | nc localhost {}", api_port));
                    l
                }
                Err(e) => {
                    log_error(
                        response_port,
                        &format!("Error al bindear API TCP en puerto {}: {}", api_port, e),
                    );
                    return;
                }
            };

            for stream in listener.incoming() {
                match stream {
                    Ok(stream) => {
                        let station = station_addr.clone();
                        let addr = local_addr.clone();
                        let port = response_port;

                        thread::spawn(move || {
                            if let Err(e) = Self::handle_api_request(stream, station, port, addr) {
                                log_error(port, &format!("Error procesando request API: {}", e));
                            }
                        });
                    }
                    Err(e) => {
                        log_error(
                            response_port,
                            &format!("Error aceptando conexión API: {}", e),
                        );
                    }
                }
            }
        });
    }

    fn handle_api_request(
        mut stream: TcpStream,
        station_addr: String,
        response_port: u16,
        local_addr: String,
    ) -> std::io::Result<()> {
        let mut reader = BufReader::new(&stream);
        let mut line = String::new();

        reader.read_line(&mut line)?;

        let request: TransactionRequest = match serde_json::from_str(&line) {
            Ok(req) => req,
            Err(e) => {
                let error_response = ApiResponse {
                    status: "error".to_string(),
                    message: format!("JSON inválido: {}", e),
                    transaction_id: None,
                };
                let response_json = serde_json::to_string(&error_response).unwrap();
                stream.write_all(format!("{}\n", response_json).as_bytes())?;
                return Ok(());
            }
        };

        log_info(
            response_port,
            &format!(
                "API: Recibida TX - Account: {}, Card: {}, Amount: ${:.2}",
                request.account_id, request.card_id, request.amount
            ),
        );

        let mut pump = Pump::new(response_port, local_addr);
        let transaction = pump.generate_transaction(
            request.account_id,
            request.card_id,
            request.amount,
            1, // station_id por defecto
        );

        match pump.send_transaction(&transaction, &station_addr) {
            Ok(_) => {
                let success_response = ApiResponse {
                    status: "ok".to_string(),
                    message: "Transacción enviada exitosamente".to_string(),
                    transaction_id: Some(transaction.transaction_id.to_string()),
                };
                let response_json = serde_json::to_string(&success_response).unwrap();
                stream.write_all(format!("{}\n", response_json).as_bytes())?;
                log_info(
                    response_port,
                    &format!(
                        "API: TX {} enviada correctamente",
                        transaction.transaction_id
                    ),
                );
            }
            Err(e) => {
                let error_response = ApiResponse {
                    status: "error".to_string(),
                    message: format!("Error enviando transacción: {}", e),
                    transaction_id: None,
                };
                let response_json = serde_json::to_string(&error_response).unwrap();
                stream.write_all(format!("{}\n", response_json).as_bytes())?;
                log_error(response_port, &format!("API: Error enviando TX - {}", e));
            }
        }

        Ok(())
    }
}

/// Estructura para deserializar requests JSON de la API
#[derive(Debug, Deserialize)]
struct TransactionRequest {
    account_id: u64,
    card_id: u64,
    amount: f64,
}

/// Estructura para respuestas JSON de la API
#[derive(Debug, Serialize)]
struct ApiResponse {
    status: String,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    transaction_id: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared_resources::messages::transaction::TransactionMessage;

    #[test]
    fn test_generate_transaction() {
        let mut pump = Pump::new(9999, "127.0.0.1:9999".to_string());
        let transaction = pump.generate_transaction(1, 555, 100.0, 10);

        assert_eq!(transaction.account_id, 1);
        assert_eq!(transaction.card_id, 555);
        assert_eq!(transaction.amount, 100.0);
        assert_eq!(transaction.station_id, 10);
        assert!(Uuid::parse_str(&transaction.transaction_id.to_string()).is_ok()); // Validate UUID
        assert_eq!(transaction.pump_address, Some("127.0.0.1:9999".to_string()));
    }

    #[test]
    fn test_send_transaction_failure() {
        let pump = Pump::new(9999, "127.0.0.1:9999".to_string());
        let transaction = TransactionMessage {
            transaction_id: Uuid::new_v4(),
            account_id: 1,
            card_id: 555,
            amount: 100.0,
            timestamp: 123456789,
            station_id: 10,
            pump_address: Some("127.0.0.1:9999".to_string()),
        };

        // Attempt to send to an invalid address
        let result = pump.send_transaction(&transaction, "invalid_address");

        assert!(result.is_err());
    }
}
