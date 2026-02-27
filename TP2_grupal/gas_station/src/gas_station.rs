//! Define la estructura y el comportamiento del actor Estación de Servicio (GasStation),
//! con todas las configuraciones parametrizables, tal como lo requiere main.rs.

use crate::types::{BulkQueue, VecDequeReExport as VecDeque};
use shared_resources::messages::protocol::{
    RegionalProtocolMessage, TransactionResponse, TransactionStatus,
};
use shared_resources::messages::transaction::TransactionMessage;
use shared_resources::utils::get_timestamp;
use std::collections::HashMap;
use std::io::{self, BufRead, BufReader, ErrorKind, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use uuid::Uuid;

/// Log informativo con timestamp y componente
fn log_info(station_id: u16, msg: &str) {
    println!(
        "[{}] [STATION #{}] [INFO] {}",
        get_timestamp(),
        station_id,
        msg
    );
}

/// Log de error con timestamp y componente
fn log_error(station_id: u16, msg: &str) {
    eprintln!(
        "[{}] [STATION #{}] [ERROR] {}",
        get_timestamp(),
        station_id,
        msg
    );
}

/// # GasStationConfig
///
/// Contiene todos los parámetros de configuración de red y límites para la Estación de Servicio.
#[derive(Clone)]
pub struct GasStationConfig {
    pub pump_listener_port: u16,
    pub regional_address: String,
    pub regional_port: u16, // Puerto para enviar el Bulk al Regional (Fase 1 y 2)
    pub bulk_limit: usize,
    pub bulk_timeout_secs: u64,
}

/// # GasStation
///
/// Actor que gestiona la recepción de ventas, el empaquetado y la coordinación del 2PC.
pub struct GasStation {
    // Estado y Configuración
    config: GasStationConfig, // Contiene todas las IPs y puertos
    buffer: Arc<Mutex<Vec<TransactionMessage>>>,
    bulk_id_counter: Arc<Mutex<u64>>,
    bulk_queue: BulkQueue,
    // Mapeo para rastrear respuestas: transaction_id -> pump_address
    transaction_routing: Arc<Mutex<HashMap<Uuid, String>>>,
}

impl GasStation {
    /// Constructor que recibe toda la configuración a través de GasStationConfig.
    pub fn new(config: GasStationConfig) -> Self {
        Self {
            config,
            buffer: Arc::new(Mutex::new(Vec::new())),
            bulk_id_counter: Arc::new(Mutex::new(0)),
            bulk_queue: Arc::new(Mutex::new(VecDeque::new())),
            transaction_routing: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Inicia los hilos de trabajo principales de la estación, usando la configuración interna.
    pub fn start(&self) -> std::thread::JoinHandle<()> {
        let config = Arc::new(self.config.clone()); // Clonamos la configuración para compartirla
        let buffer_listener = self.buffer.clone();
        let buffer_packager = self.buffer.clone();
        let bulk_queue_packager = self.bulk_queue.clone();
        let bulk_queue_sender = self.bulk_queue.clone();
        let bulk_id_counter = self.bulk_id_counter.clone();
        let transaction_routing_listener = self.transaction_routing.clone();
        let transaction_routing_sender = self.transaction_routing.clone();

        std::thread::spawn(move || {
            // --- Hilo 1: Receptor de Conexiones de Surtidores (Ventas) ---
            let config_listener = config.clone();
            let station_id = config_listener.pump_listener_port;
            let listener_handle = thread::spawn(move || {
                Self::listen_for_pumps(station_id, buffer_listener, transaction_routing_listener);
            });

            // --- Hilo 2: Empaquetador de Buffer (Decisor de Bulk) ---
            let config_packager = config.clone();
            let station_id_packager = config_packager.pump_listener_port;
            let packager_handle = thread::spawn(move || {
                let mut last_bulk_time = Instant::now();
                let bulk_timeout = Duration::from_secs(config_packager.bulk_timeout_secs);
                let bulk_limit = config_packager.bulk_limit;

                loop {
                    let mut buffer = buffer_packager.lock().unwrap();
                    let should_package = buffer.len() >= bulk_limit
                        || (!buffer.is_empty() && last_bulk_time.elapsed() >= bulk_timeout);

                    if should_package {
                        let mut bulk_id = bulk_id_counter.lock().unwrap();
                        *bulk_id += 1;

                        let transactions = buffer.drain(..).collect::<Vec<_>>();
                        let mut queue = bulk_queue_packager.lock().unwrap();
                        queue.push_back((*bulk_id, transactions));
                        log_info(
                            station_id_packager,
                            &format!(
                                "PACKAGER - Empaquetado Bulk {}. Tamaño cola: {}",
                                *bulk_id,
                                queue.len()
                            ),
                        );

                        last_bulk_time = Instant::now();
                    }
                    drop(buffer);
                    thread::sleep(Duration::from_millis(100));
                }
            });

            // --- Hilo 3: Enviador de Bulks (Coordinador 2PC) ---
            let config_sender = config.clone();
            let station_id_sender = config_sender.pump_listener_port;
            let _sender_handle = thread::spawn(move || {
                loop {
                    let front_item = {
                        let queue = bulk_queue_sender.lock().unwrap();
                        queue.front().cloned()
                    };

                    if let Some((bulk_id, transactions)) = front_item {
                        match Self::send_bulk_to_regional(
                            bulk_id,
                            &transactions,
                            &config_sender, // Pasamos la configuración para acceder a IPs/Puertos
                            &transaction_routing_sender,
                        ) {
                            Ok(true) => {
                                // 2PC Exitoso
                                log_info(
                                    station_id_sender,
                                    &format!("SENDER - 2PC exitoso para Bulk {}", bulk_id),
                                );
                                let mut queue = bulk_queue_sender.lock().unwrap();
                                queue.pop_front();
                            }
                            Ok(false) => {
                                // VOTE_ABORT
                                log_error(
                                    station_id_sender,
                                    &format!(
                                        "SENDER - Regional ABORTÓ Bulk {}. Se descarta el Bulk",
                                        bulk_id
                                    ),
                                );
                                let mut queue = bulk_queue_sender.lock().unwrap();
                                queue.pop_front();
                            }
                            Err(e) => {
                                log_error(
                                    station_id_sender,
                                    &format!(
                                        "SENDER - Falló la conexión/2PC para Bulk {}: {}. Reintentando...",
                                        bulk_id, e
                                    ),
                                );
                                thread::sleep(Duration::from_secs(5));
                            }
                        }
                    } else {
                        thread::sleep(Duration::from_millis(100));
                    }
                }
            });

            // Esperar a que los hilos terminen (solo esperamos los hilos principales)
            listener_handle.join().unwrap();
            packager_handle.join().unwrap();
            // Note: sender_handle runs indefinitely so we don't join it here
        })
    }

    // --- FUNCIONALIDADES PÚBLICAS Y AUXILIARES ---

    /// Abre el listener TCP y maneja las conexiones de los surtidores de forma concurrente.
    fn listen_for_pumps(
        station_id: u16,
        buffer: Arc<Mutex<Vec<TransactionMessage>>>,
        routing: Arc<Mutex<HashMap<Uuid, String>>>,
    ) {
        let address = format!("0.0.0.0:{}", station_id);
        let listener = TcpListener::bind(&address).unwrap_or_else(|_| {
            panic!("Fallo al bindear el listener de surtidores en {}", address)
        });
        log_info(
            station_id,
            &format!("LISTENER - Escuchando surtidores en puerto {}", station_id),
        );

        for stream in listener.incoming() {
            match stream {
                Ok(stream) => {
                    let buffer_clone = buffer.clone();
                    let routing_clone = routing.clone();
                    thread::spawn(move || {
                        if let Err(e) = Self::handle_pump_connection(
                            stream,
                            buffer_clone,
                            routing_clone,
                            station_id,
                        ) {
                            log_error(
                                station_id,
                                &format!("LISTENER - Error procesando conexión de surtidor: {}", e),
                            );
                        }
                    });
                }
                Err(e) => {
                    log_error(
                        station_id,
                        &format!("LISTENER - Error al aceptar conexión: {}", e),
                    );
                }
            }
        }
    }

    /// Coordina el Two-Phase Commit (2PC) con el Servidor Regional.
    /// Retorna Ok(true) si el commit fue exitoso, Ok(false) si fue VOTE_ABORT, Err si hay fallo de conexión.
    fn send_bulk_to_regional(
        bulk_id: u64,
        transactions: &[TransactionMessage],
        config: &GasStationConfig, // Configuración parametrizable
        routing: &Arc<Mutex<HashMap<Uuid, String>>>,
    ) -> io::Result<bool> {
        let address = format!("{}:{}", config.regional_address, config.regional_port);
        let station_id = config.pump_listener_port;

        // --- FASE 1: VOTE_REQUEST (Conexión y Envío) ---
        // NOTA: Usaremos una sola conexión para la Fase 1 y su respuesta.
        let mut stream = TcpStream::connect(&address)?;
        log_info(
            station_id,
            &format!("2PC - Conectado a RegionalServer en {}", address),
        );

        let vote_request = RegionalProtocolMessage::BulkTransaction {
            bulk_id,
            transactions: transactions.to_vec(),
        };

        let serialized_request =
            serde_json::to_string(&vote_request).map_err(|e| io::Error::other(e.to_string()))?;

        // Envío del PREPARE + Delimitador Newline (\n)
        stream.write_all(serialized_request.as_bytes())?;
        stream.write_all(b"\n")?;
        stream.flush()?;
        log_info(
            station_id,
            &format!(
                "2PC Fase 1 - Enviado Bulk {} (VOTE_REQUEST). Esperando voto...",
                bulk_id
            ),
        );

        // --- RECEPCIÓN del VOTO ---
        // Usamos BufReader para leer la respuesta delimitada por \n
        let mut reader = BufReader::new(stream.try_clone()?);
        let mut response_string = String::new();

        // Leemos la respuesta del voto (que el Regional envió con \n)
        if reader.read_line(&mut response_string)? == 0 {
            return Err(io::Error::new(
                ErrorKind::ConnectionReset,
                "Regional cerró la conexión sin enviar voto.",
            ));
        }

        // Cerramos el lado de escritura de la Estación (ya enviamos el PREPARE)
        // El Regional debe cerrar su lado de escritura después de enviar el voto.
        match stream.shutdown(std::net::Shutdown::Write) {
            Ok(_) => {}
            Err(e) => log_error(
                station_id,
                &format!("Error al cerrar lado de escritura del stream: {}", e),
            ),
        }

        let regional_response: RegionalProtocolMessage = serde_json::from_str(&response_string)
            .map_err(|e| {
                io::Error::new(
                    ErrorKind::InvalidData,
                    format!("Error deserializando voto: {}", e),
                )
            })?;

        // 3. Procesamiento del Voto
        match regional_response {
            RegionalProtocolMessage::BulkPrepareResponse {
                bulk_id: _response_bulk_id,
                vote: true,
                accepted_transactions,
                rejected_transactions,
            } => {
                let original_count = transactions.len();
                let accepted_count = accepted_transactions.len();
                let rejected_count = rejected_transactions.len();

                log_info(
                    station_id,
                    &format!(
                        "2PC Fase 1 - VOTE_COMMIT - {}/{} aceptadas, {} rechazadas",
                        accepted_count, original_count, rejected_count
                    ),
                );

                // Notificar a pumps las transacciones rechazadas inmediatamente
                for (rejected_tx, status) in &rejected_transactions {
                    Self::notify_pump_transaction_result(
                        rejected_tx,
                        status.clone(),
                        routing,
                        station_id,
                    );
                }

                // --- FASE 2: GLOBAL_COMMIT ---
                let commit_msg = RegionalProtocolMessage::GlobalCommit { bulk_id };
                let serialized_commit = serde_json::to_string(&commit_msg)
                    .map_err(|e| io::Error::other(e.to_string()))?;

                // [A] Reestablecemos la conexión para la Fase 2 (ABRE NUEVO STREAM)
                let mut stream_commit = TcpStream::connect(&address)?;

                // 4. Envío del COMMIT + Delimitador Newline (\n)
                stream_commit.write_all(serialized_commit.as_bytes())?;
                stream_commit.write_all(b"\n")?;
                stream_commit.flush()?;

                log_info(
                    station_id,
                    &format!(
                        "2PC Fase 2 - Enviado GLOBAL_COMMIT para Bulk {}. Esperando ACK...",
                        bulk_id
                    ),
                );

                // 5. RECEPCIÓN del ACK (Lectura Bloqueante en el NUEVO stream)
                let mut ack_reader = BufReader::new(stream_commit.try_clone()?);
                let mut ack_string = String::new();

                // Leemos hasta el delimitador \n que el Regional debe enviar.
                if ack_reader.read_line(&mut ack_string)? == 0 {
                    return Err(io::Error::new(
                        ErrorKind::ConnectionReset,
                        "ACK no recibido: Regional cerró la conexión prematuramente.",
                    ));
                }

                let ack_response: RegionalProtocolMessage = serde_json::from_str(&ack_string)
                    .map_err(|e| {
                        io::Error::new(
                            ErrorKind::InvalidData,
                            format!("Error deserializando ACK: {}", e),
                        )
                    })?;

                // 6. Validamos el ACK
                match ack_response {
                    RegionalProtocolMessage::BulkCommittedAck { bulk_id: ack_id }
                        if ack_id == bulk_id =>
                    {
                        log_info(
                            station_id,
                            &format!("2PC - ACK recibido. Lote {} confirmado", bulk_id),
                        );

                        // Notificar a los pumps las transacciones aceptadas
                        for accepted_tx in &accepted_transactions {
                            Self::notify_pump_transaction_result(
                                accepted_tx,
                                TransactionStatus::Accepted,
                                routing,
                                station_id,
                            );
                        }

                        // 7. Cerramos la conexión de COMMIT
                        stream_commit.shutdown(std::net::Shutdown::Both)?;
                        Ok(true)
                    }
                    _ => {
                        stream_commit.shutdown(std::net::Shutdown::Both)?;
                        Err(io::Error::other(format!(
                            "Respuesta inesperada en lugar de ACK: {:?}",
                            ack_response
                        )))
                    }
                }
            }
            RegionalProtocolMessage::BulkPrepareResponse {
                bulk_id: _,
                vote: false,
                accepted_transactions: _,
                rejected_transactions,
            } => {
                // Manejar ABORT explícitamente. La conexión original ya cerró la escritura.
                log_error(
                    station_id,
                    &format!(
                        "2PC Fase 1 - Regional votó ABORT para Bulk {}. Todas rechazadas",
                        bulk_id
                    ),
                );

                // Notificar a todos los pumps que sus transacciones fueron rechazadas
                for (rejected_tx, status) in &rejected_transactions {
                    Self::notify_pump_transaction_result(
                        rejected_tx,
                        status.clone(),
                        routing,
                        station_id,
                    );
                }

                Ok(false) // VOTE_ABORT
            }
            _ => {
                // Manejar cualquier otro mensaje inesperado recibido en FASE 1
                Err(io::Error::other(
                    "Mensaje de protocolo inesperado en Fase 1",
                ))
            }
        }
    }

    /// Maneja la conexión individual de un surtidor para leer y deserializar la TransactionMessage.
    fn handle_pump_connection(
        stream: TcpStream,
        buffer: Arc<Mutex<Vec<TransactionMessage>>>,
        routing: Arc<Mutex<HashMap<Uuid, String>>>,
        station_id: u16,
    ) -> std::io::Result<()> {
        // Obtener la dirección del pump antes de leer
        let pump_addr = stream
            .peer_addr()
            .map(|addr| addr.to_string())
            .unwrap_or_else(|_| "Unknown".to_string());

        let mut stream_clone = stream;
        let mut data = Vec::new();
        stream_clone.read_to_end(&mut data)?;

        let mut transaction: TransactionMessage = serde_json::from_slice(&data).map_err(|e| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Error deserializando TX: {}", e),
            )
        })?;

        log_info(
            station_id,
            &format!(
                "LISTENER - Transacción recibida: {} desde {}",
                transaction.transaction_id, pump_addr
            ),
        );

        // Guardar la dirección del pump si fue proporcionada, sino usar peer_addr
        let pump_address = transaction.pump_address.clone().unwrap_or(pump_addr);
        routing
            .lock()
            .unwrap()
            .insert(transaction.transaction_id, pump_address.clone());

        // Asegurar que el pump_address esté en la transacción
        transaction.pump_address = Some(pump_address);

        buffer.lock().unwrap().push(transaction);
        Ok(())
    }

    /// Notifica al pump el resultado de su transacción
    fn notify_pump_transaction_result(
        transaction: &TransactionMessage,
        status: TransactionStatus,
        routing: &Arc<Mutex<HashMap<Uuid, String>>>,
        station_id: u16,
    ) {
        let pump_addr = if let Some(addr) = &transaction.pump_address {
            addr.clone()
        } else {
            // Intentar obtener del routing map
            let routing_map = routing.lock().unwrap();
            if let Some(addr) = routing_map.get(&transaction.transaction_id) {
                addr.clone()
            } else {
                log_error(
                    station_id,
                    &format!(
                        "NOTIFY - No se pudo determinar dirección del pump para TX {}",
                        transaction.transaction_id
                    ),
                );
                return;
            }
        };

        let message = match &status {
            TransactionStatus::Accepted => "Transacción aprobada y confirmada".to_string(),
            TransactionStatus::RejectedLimitExceeded {
                available,
                attempted,
            } => {
                format!(
                    "Rechazada: límite de cuenta excedido (disponible: {:.2}, intentado: {:.2})",
                    available, attempted
                )
            }
            TransactionStatus::RejectedCardLimitExceeded {
                card_limit,
                attempted,
            } => {
                format!(
                    "Rechazada: límite de tarjeta excedido (límite: {:.2}, intentado: {:.2})",
                    card_limit, attempted
                )
            }
            TransactionStatus::RejectedAccountNotFound => {
                "Rechazada: cuenta no encontrada".to_string()
            }
            TransactionStatus::RejectedCardNotFound => {
                "Rechazada: tarjeta no encontrada o no asociada a la cuenta".to_string()
            }
            TransactionStatus::RejectedAccountInactive => {
                "Rechazada: cuenta inactiva o bloqueada".to_string()
            }
        };

        let response = TransactionResponse {
            transaction_id: transaction.transaction_id,
            status: status.clone(),
            message,
        };

        // Intentar enviar la notificación al pump
        match TcpStream::connect(&pump_addr) {
            Ok(mut pump_stream) => {
                if let Ok(json) = serde_json::to_string(&response) {
                    let _ = pump_stream.write_all(json.as_bytes());
                    let _ = pump_stream.write_all(b"\n");
                    let _ = pump_stream.flush();
                    log_info(
                        station_id,
                        &format!(
                            "NOTIFY - Respuesta enviada a pump {}: TX {}: {:?}",
                            pump_addr, transaction.transaction_id, response.status
                        ),
                    );
                }
            }
            Err(e) => {
                log_error(
                    station_id,
                    &format!(
                        "NOTIFY - Error conectando a pump {} para TX {}: {}",
                        pump_addr, transaction.transaction_id, e
                    ),
                );
            }
        }

        // Limpiar del routing map
        routing.lock().unwrap().remove(&transaction.transaction_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared_resources::messages::protocol::TransactionStatus;
    use shared_resources::messages::transaction::TransactionMessage;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};
    use uuid::Uuid;

    fn create_test_transaction() -> TransactionMessage {
        TransactionMessage {
            transaction_id: Uuid::new_v4(),
            account_id: 1,
            card_id: 1,
            amount: 100.0,
            timestamp: 0,
            station_id: 1,
            pump_address: Some("127.0.0.1:8080".to_string()),
        }
    }

    #[test]
    fn handle_pump_connection_adds_transaction_to_buffer() {
        use std::net::TcpListener;

        let buffer = Arc::new(Mutex::new(Vec::new()));
        let routing = Arc::new(Mutex::new(HashMap::new()));
        let transaction = create_test_transaction();
        let serialized_transaction = serde_json::to_vec(&transaction).unwrap();

        // Bind a TcpListener to a dynamic port
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();

        // Spawn a thread to accept the connection and send the serialized transaction
        std::thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                stream.write_all(&serialized_transaction).unwrap();
            }
        });

        // Connect to the dynamically assigned port
        let stream = TcpStream::connect(addr).unwrap();

        let result = GasStation::handle_pump_connection(
            stream.try_clone().unwrap(),
            buffer.clone(),
            routing.clone(),
            8080,
        );

        assert!(result.is_ok());
        let buffer_guard = buffer.lock().unwrap();
        assert_eq!(buffer_guard.len(), 1);
        assert_eq!(buffer_guard[0].transaction_id, transaction.transaction_id);
    }

    #[test]
    fn notify_pump_transaction_result_sends_accepted_status() {
        let routing = Arc::new(Mutex::new(HashMap::new()));
        let transaction = create_test_transaction();
        let status = TransactionStatus::Accepted;

        GasStation::notify_pump_transaction_result(&transaction, status.clone(), &routing, 8080);

        // No assertion here as the function logs the result and sends data over TCP.
        // This test ensures no panics or errors occur during execution.
    }

    #[test]
    fn send_bulk_to_regional_handles_vote_abort() {
        use std::io::{BufRead, BufReader, Write};
        use std::net::TcpListener;
        use std::thread;

        // Mock Regional Server
        let regional_server = TcpListener::bind("127.0.0.1:9090").unwrap();
        thread::spawn(move || {
            if let Ok((mut stream, _)) = regional_server.accept() {
                let mut buffer = String::new();
                let mut reader = BufReader::new(&stream);
                reader.read_line(&mut buffer).unwrap();

                // Simulate VOTE_ABORT response
                let response =
                    serde_json::to_string(&RegionalProtocolMessage::BulkPrepareResponse {
                        bulk_id: 1,
                        vote: false,
                        accepted_transactions: vec![],
                        rejected_transactions: vec![],
                    })
                    .unwrap();
                writeln!(stream, "{}", response).unwrap();
            }
        });

        // Test Configuration
        let config = GasStationConfig {
            pump_listener_port: 8080,
            regional_address: "127.0.0.1".to_string(),
            regional_port: 9090,
            bulk_limit: 10,
            bulk_timeout_secs: 5,
        };
        let routing = Arc::new(Mutex::new(HashMap::new()));
        let transactions = vec![create_test_transaction()];

        // Call the function and assert
        let result = GasStation::send_bulk_to_regional(1, &transactions, &config, &routing);

        assert!(result.is_ok());
        assert!(!result.unwrap());
    }
}
