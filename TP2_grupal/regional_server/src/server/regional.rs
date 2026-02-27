use std::io::{self, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc::{self, Sender};
use std::thread;
use std::time::Duration;

use shared_resources::messages::protocol::RegionalBatchMessage;
use shared_resources::messages::transaction::TransactionMessage;

use crate::server::accounts::AccountsManager;
use crate::server::client_handler::ClientHandler;

pub struct RegionalServer {
    name: String,          // Nombre de la región (ej: "CABA")
    region_id: Option<u8>, // ID asignado por el Central (None hasta registrarse)
    port: u16,
    central_sync_port: u16, // Puerto para recibir actualizaciones del Central
    accounts: AccountsManager,
    central_sender: Sender<Vec<TransactionMessage>>,
    central_addresses: Vec<String>,
}

impl RegionalServer {
    pub fn new(
        name: String,
        port: u16,
        central_sync_port: u16,
        central_addresses: Vec<String>,
    ) -> Self {
        println!(
            "[Regional '{}'] Inicializando con {} réplicas centrales conocidas",
            name,
            central_addresses.len()
        );

        let (tx, rx) = mpsc::channel::<Vec<TransactionMessage>>();
        let server_name = name.clone();
        let addresses = central_addresses.clone();

        thread::spawn(move || {
            println!("[Regional '{}'] CentralReporter - Iniciando", server_name);

            let mut current_stream: Option<TcpStream> = None;
            let mut addr_index = 0;
            let region_id: u8 = 0; // Se actualizará después del registro

            for confirmed_batch in rx {
                println!(
                    "[Regional '{}'] CentralReporter - Procesando lote de {} ventas",
                    server_name,
                    confirmed_batch.len()
                );

                let msg = RegionalBatchMessage {
                    regional_id: region_id, // Usamos el ID asignado por el Central
                    transactions: confirmed_batch,
                };

                let json_payload = match serde_json::to_string(&msg) {
                    Ok(j) => j,
                    Err(e) => {
                        eprintln!(
                            "[Regional '{}'] CentralReporter - Error fatal serializando: {}",
                            server_name, e
                        );
                        continue;
                    }
                };

                let payload_bytes = json_payload.as_bytes();
                let packet_size = (payload_bytes.len() + 1) as u64; // +1 para el código de operación
                let size_bytes = packet_size.to_be_bytes();

                // Código de operación para batch del regional (definido en shared_resources)
                const REGIONAL_BATCH_CODE: u8 = 6;

                loop {
                    if current_stream.is_none() {
                        let start_index = addr_index;
                        let mut connected = false;

                        loop {
                            let target = &addresses[addr_index];
                            println!(
                                "[Regional '{}'] CentralReporter - Intentando conectar a Central {} ({})",
                                server_name, addr_index, target
                            );

                            match TcpStream::connect(target) {
                                Ok(s) => {
                                    println!(
                                        "[Regional '{}'] CentralReporter - Conexión establecida con {}",
                                        server_name, target
                                    );
                                    current_stream = Some(s);
                                    connected = true;
                                    break;
                                }
                                Err(_) => {
                                    eprintln!(
                                        "[Regional '{}'] CentralReporter - Falló {} - probando siguiente",
                                        server_name, target
                                    );
                                    addr_index = (addr_index + 1) % addresses.len();
                                    if addr_index == start_index {
                                        break;
                                    }
                                }
                            }
                        }

                        if !connected {
                            eprintln!(
                                "[Regional '{}'] CentralReporter - Ninguna réplica del central responde - reintentando",
                                server_name
                            );
                            thread::sleep(Duration::from_secs(5));
                            continue;
                        }
                    }

                    if let Some(stream) = current_stream.as_mut() {
                        // Enviar usando el protocolo frame esperado por el Central Server:
                        // 1. 8 bytes: tamaño del paquete (código + payload)
                        // 2. 1 byte: código de operación
                        // 3. N bytes: payload JSON
                        let send_result = stream
                            .write_all(&size_bytes)
                            .and_then(|_| stream.write_all(&[REGIONAL_BATCH_CODE]))
                            .and_then(|_| stream.write_all(payload_bytes))
                            .and_then(|_| stream.flush());

                        if let Err(e) = send_result {
                            eprintln!(
                                "[Regional '{}'] CentralReporter - Error enviando ({}) - se cayó el central",
                                server_name, e
                            );
                            current_stream = None;
                            addr_index = (addr_index + 1) % addresses.len();
                            thread::sleep(Duration::from_millis(500));
                            continue;
                        }

                        // Leer respuesta del Central Server (protocolo: 8 bytes tamaño + payload)
                        let mut size_buf = [0u8; 8];
                        if let Err(e) = stream.read_exact(&mut size_buf) {
                            eprintln!(
                                "[Regional '{}'] CentralReporter - Error leyendo respuesta ({})",
                                server_name, e
                            );
                            current_stream = None;
                            addr_index = (addr_index + 1) % addresses.len();
                            thread::sleep(Duration::from_millis(500));
                            continue;
                        }

                        let response_size = u64::from_be_bytes(size_buf) as usize;
                        if response_size == 0 {
                            println!(
                                "[Regional '{}'] CentralReporter - Lote confirmado (sin respuesta)",
                                server_name
                            );
                            break;
                        }

                        let mut response_buf = vec![0u8; response_size];
                        if let Err(e) = stream.read_exact(&mut response_buf) {
                            eprintln!(
                                "[Regional '{}'] CentralReporter - Error leyendo payload de respuesta ({})",
                                server_name, e
                            );
                            current_stream = None;
                            addr_index = (addr_index + 1) % addresses.len();
                            thread::sleep(Duration::from_millis(500));
                            continue;
                        }

                        // Deserializar DataResponseMessage
                        use shared_resources::messages::data_response::DataResponseMessage;
                        use shared_resources::utils::FAILURE;

                        let response = match DataResponseMessage::deserialize(&response_buf) {
                            Ok(r) => r,
                            Err(e) => {
                                eprintln!(
                                    "[Regional '{}'] CentralReporter - Error deserializando respuesta: {}",
                                    server_name, e
                                );
                                current_stream = None;
                                break; // Asumir éxito si no podemos parsear
                            }
                        };

                        // Verificar si es un FAILURE (redirección)
                        if response.status() == FAILURE {
                            if let Some(leader_bytes) = response.piggyback() {
                                let leader_addr =
                                    String::from_utf8_lossy(&leader_bytes).to_string();
                                println!(
                                    "[Regional '{}'] CentralReporter - Nodo no es líder, redirigiendo a {}",
                                    server_name, leader_addr
                                );

                                // Intentar conectar directamente con el líder
                                match TcpStream::connect(&leader_addr) {
                                    Ok(new_stream) => {
                                        println!(
                                            "[Regional '{}'] CentralReporter - Conectado al líder en {}",
                                            server_name, leader_addr
                                        );
                                        current_stream = Some(new_stream);
                                        // Reintentará enviar en la siguiente iteración del loop
                                        continue;
                                    }
                                    Err(e) => {
                                        eprintln!(
                                            "[Regional '{}'] CentralReporter - No pudo conectar al líder {}: {}",
                                            server_name, leader_addr, e
                                        );
                                        current_stream = None;
                                        addr_index = (addr_index + 1) % addresses.len();
                                        thread::sleep(Duration::from_millis(500));
                                        continue;
                                    }
                                }
                            } else {
                                eprintln!(
                                    "[Regional '{}'] CentralReporter - FAILURE sin dirección de líder",
                                    server_name
                                );
                                current_stream = None;
                                addr_index = (addr_index + 1) % addresses.len();
                                thread::sleep(Duration::from_millis(500));
                                continue;
                            }
                        }

                        // Respuesta exitosa
                        println!(
                            "[Regional '{}'] CentralReporter - Lote confirmado por central",
                            server_name
                        );
                        break;
                    }
                }
            }
        });

        Self {
            name,
            region_id: None, // Se asignará tras el registro
            port,
            central_sync_port,
            accounts: AccountsManager::new(),
            central_sender: tx,
            central_addresses,
        }
    }

    pub fn run(&self) -> io::Result<()> {
        let address = format!("0.0.0.0:{}", self.port);
        let listener = TcpListener::bind(&address)?;

        println!(
            "[Regional '{}'] Escuchando Estaciones de Servicio en puerto {}",
            self.name, self.port
        );

        let regional_name = self.name.clone();
        let regional_id = self.region_id.unwrap_or(0);

        for stream in listener.incoming() {
            match stream {
                Ok(stream) => {
                    let accounts_clone = self.accounts.clone();
                    let sender_clone = self.central_sender.clone();

                    thread::spawn(move || {
                        let mut handler =
                            ClientHandler::new(stream, accounts_clone, sender_clone, regional_id);
                        handler.run();
                    });
                }
                Err(e) => {
                    eprintln!(
                        "[Regional '{}'] Error al aceptar conexión: {}",
                        regional_name, e
                    );
                }
            }
        }

        Ok(())
    }

    /// Registra este Regional Server en el Central Server
    /// Retorna (region_id, cuentas_iniciales)
    pub fn register_with_central(
        &mut self,
    ) -> io::Result<(
        u8,
        Vec<shared_resources::messages::protocol::AccountSyncData>,
    )> {
        use shared_resources::messages::protocol::{
            REGISTER_REGIONAL, RegisterRegionalMessage, RegisterRegionalResponse,
        };
        use std::io::Read;

        println!(
            "[Regional '{}'] Registrándose en el Central Server...",
            self.name
        );

        // Intentar con cada dirección del Central hasta tener éxito
        for central_addr in self.central_addresses.iter() {
            println!(
                "[Regional '{}'] Conectando a Central en {}...",
                self.name, central_addr
            );

            let mut stream = match TcpStream::connect(central_addr) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!(
                        "[Regional '{}'] Error conectando a {}: {}",
                        self.name, central_addr, e
                    );
                    continue;
                }
            };

            // Crear mensaje de registro
            let reg_msg = RegisterRegionalMessage::new(
                self.name.clone(),
                format!("{}:{}", get_local_ip()?, self.central_sync_port),
            );

            // Enviar con protocolo de framing
            let payload = serde_json::to_vec(&reg_msg).unwrap();
            let framed = frame_payload_with_code(REGISTER_REGIONAL, &payload);

            if stream.write_all(&framed).is_err() || stream.flush().is_err() {
                eprintln!(
                    "[Regional '{}'] Error enviando registro a {}",
                    self.name, central_addr
                );
                continue;
            }

            // Leer respuesta (protocolo: 8 bytes tamaño + payload)
            // El payload contiene: 1 byte código + datos serializados
            let mut size_buf = [0u8; 8];
            if stream.read_exact(&mut size_buf).is_err() {
                eprintln!(
                    "[Regional '{}'] Error leyendo tamaño de respuesta de {}",
                    self.name, central_addr
                );
                continue;
            }

            let total_size = u64::from_be_bytes(size_buf) as usize;
            if total_size == 0 {
                eprintln!(
                    "[Regional '{}'] Respuesta vacía de {}",
                    self.name, central_addr
                );
                continue;
            }

            let mut response_buf = vec![0u8; total_size];
            if stream.read_exact(&mut response_buf).is_err() {
                eprintln!(
                    "[Regional '{}'] Error leyendo payload de respuesta",
                    self.name
                );
                continue;
            }

            // El primer byte es el código de operación, el resto es el JSON
            if response_buf.len() < 2 {
                eprintln!(
                    "[Regional '{}'] Respuesta muy corta (solo código, sin datos)",
                    self.name
                );
                continue;
            }

            let _response_code = response_buf[0];
            let response_payload = &response_buf[1..];

            // Deserializar la respuesta JSON
            let response: RegisterRegionalResponse = match serde_json::from_slice(response_payload)
            {
                Ok(r) => r,
                Err(e) => {
                    eprintln!(
                        "[Regional '{}'] Error deserializando respuesta: {}",
                        self.name, e
                    );
                    eprintln!(
                        "[Regional '{}'] Payload recibido ({} bytes): {:?}",
                        self.name,
                        response_payload.len(),
                        String::from_utf8_lossy(response_payload)
                    );
                    continue;
                }
            };

            if !response.success {
                // Verificar si es una redirección al líder
                if let Some(leader_addr) = response.leader_address {
                    println!(
                        "[Regional '{}'] Nodo no es líder, intentando directamente con líder en {}...",
                        self.name, leader_addr
                    );

                    // Intentar directamente con el líder
                    println!(
                        "[Regional '{}'] Conectando a líder en {}...",
                        self.name, leader_addr
                    );

                    let mut leader_stream = match TcpStream::connect(&leader_addr) {
                        Ok(s) => s,
                        Err(e) => {
                            eprintln!(
                                "[Regional '{}'] Error conectando al líder {}: {}",
                                self.name, leader_addr, e
                            );
                            continue; // Intentar con siguiente de la lista
                        }
                    };

                    // Reenviar mensaje de registro al líder
                    let reg_msg = RegisterRegionalMessage::new(
                        self.name.clone(),
                        format!("{}:{}", get_local_ip()?, self.central_sync_port),
                    );

                    let payload = serde_json::to_vec(&reg_msg).unwrap();
                    let framed = frame_payload_with_code(REGISTER_REGIONAL, &payload);

                    if leader_stream.write_all(&framed).is_err() || leader_stream.flush().is_err() {
                        eprintln!(
                            "[Regional '{}'] Error enviando registro al líder",
                            self.name
                        );
                        continue;
                    }

                    // Leer respuesta del líder
                    let mut size_buf = [0u8; 8];
                    if leader_stream.read_exact(&mut size_buf).is_err() {
                        eprintln!(
                            "[Regional '{}'] Error leyendo respuesta del líder",
                            self.name
                        );
                        continue;
                    }

                    let total_size = u64::from_be_bytes(size_buf) as usize;
                    let mut response_buf = vec![0u8; total_size];
                    if leader_stream.read_exact(&mut response_buf).is_err() {
                        eprintln!("[Regional '{}'] Error leyendo payload del líder", self.name);
                        continue;
                    }

                    let leader_response: RegisterRegionalResponse =
                        match serde_json::from_slice(&response_buf[1..]) {
                            Ok(r) => r,
                            Err(e) => {
                                eprintln!(
                                    "[Regional '{}'] Error deserializando respuesta del líder: {}",
                                    self.name, e
                                );
                                continue;
                            }
                        };

                    if !leader_response.success {
                        eprintln!(
                            "[Regional '{}'] Líder también rechazó: {:?}",
                            self.name, leader_response.error_message
                        );
                        continue;
                    }

                    // ¡Éxito con el líder!
                    self.region_id = Some(leader_response.region_id);
                    println!(
                        "[Regional '{}'] [INFO] Registered with leader - ID {} - {} accounts",
                        self.name,
                        leader_response.region_id,
                        leader_response.accounts.len()
                    );
                    return Ok((leader_response.region_id, leader_response.accounts));
                }

                // Si no es redirección, es un error real
                eprintln!(
                    "[Regional '{}'] [ERROR] Registration rejected: {:?}",
                    self.name, response.error_message
                );
                eprintln!(
                    "[Regional '{}'] [INFO] Available regions: {:?}",
                    self.name, response.available_regions
                );
                continue; // Intentar con siguiente dirección
            }

            // ¡Éxito!
            self.region_id = Some(response.region_id);
            println!(
                "[Regional '{}'] [INFO] Registered with ID {} - received {} accounts",
                self.name,
                response.region_id,
                response.accounts.len()
            );

            return Ok((response.region_id, response.accounts));
        }

        Err(io::Error::new(
            io::ErrorKind::NotConnected,
            "No se pudo conectar a ningún Central Server",
        ))
    }

    /// Carga una cuenta desde los datos de sincronización
    pub fn load_account_from_sync(
        &self,
        account: shared_resources::messages::protocol::AccountSyncData,
    ) {
        self.accounts.update_account_with_state(
            account.account_id,
            account.username,
            account.display_name,
            account.limit,
            account.active,
        );

        for (card_id, card_limit) in account.cards {
            self.accounts
                .add_card_to_account(account.account_id, card_id, card_limit);
        }
    }

    /// Inicia el listener para recibir actualizaciones del Central Server
    pub fn start_central_sync_listener(&self) -> io::Result<()> {
        let sync_address = format!("0.0.0.0:{}", self.central_sync_port);
        let listener = TcpListener::bind(&sync_address)?;

        println!(
            "[Regional '{}'] Escuchando sincronización del Central en puerto {}",
            self.name, self.central_sync_port
        );

        let regional_id = self.region_id.unwrap_or(0);
        let regional_name = self.name.clone();
        let accounts_clone = self.accounts.clone();

        thread::spawn(move || {
            for stream in listener.incoming() {
                match stream {
                    Ok(stream) => {
                        let accounts = accounts_clone.clone();
                        let name = regional_name.clone();
                        thread::spawn(move || {
                            if let Err(e) = handle_central_sync(stream, accounts, regional_id) {
                                eprintln!("[Regional '{}'] Error procesando sync: {}", name, e);
                            }
                        });
                    }
                    Err(e) => {
                        eprintln!(
                            "[Regional '{}'] Error aceptando conexión de sync: {}",
                            regional_name, e
                        );
                    }
                }
            }
        });

        Ok(())
    }
}

/// Maneja mensajes de sincronización desde el Central Server
fn handle_central_sync(
    stream: TcpStream,
    accounts: AccountsManager,
    regional_id: u8,
) -> io::Result<()> {
    use shared_resources::messages::protocol::CentralSyncMessage;
    use std::io::BufRead;
    use std::io::BufReader;

    let reader = BufReader::new(stream);

    for line in reader.lines() {
        let line = line?;

        // Deserializar el mensaje de sincronización
        let sync_msg: CentralSyncMessage = match serde_json::from_str(&line) {
            Ok(msg) => msg,
            Err(e) => {
                eprintln!(
                    "[Regional #{}] Error deserializando sync: {}",
                    regional_id, e
                );
                continue;
            }
        };

        // Aplicar la actualización
        match sync_msg {
            CentralSyncMessage::AccountUpdate {
                account_id,
                username,
                display_name,
                region_name,
                region_limit,
                active,
            } => {
                println!(
                    "[Regional #{}] Sync: Actualizando cuenta {} (@{}: {}) en región '{}', límite={:.2}, activa={}",
                    regional_id,
                    account_id,
                    username,
                    display_name,
                    region_name,
                    region_limit,
                    active
                );
                accounts.update_account_with_state(
                    account_id,
                    username,
                    display_name,
                    region_limit,
                    active,
                );
            }

            CentralSyncMessage::CardUpdate {
                account_id,
                card_id,
                card_limit,
            } => {
                println!(
                    "[Regional #{}] Sync: Actualizando tarjeta {} de cuenta {}, límite={:.2}",
                    regional_id, card_id, account_id, card_limit
                );
                accounts.add_card_to_account(account_id, card_id, card_limit);
            }

            CentralSyncMessage::AccountDeleted { account_id } => {
                println!(
                    "[Regional #{}] Sync: Desactivando cuenta {}",
                    regional_id, account_id
                );
                accounts.deactivate_account(account_id);
            }

            CentralSyncMessage::RemoveAccount {
                account_id,
                region_name,
            } => {
                println!(
                    "[Regional #{}] Sync: Removiendo cuenta {} del regional (región '{}' removida)",
                    regional_id, account_id, region_name
                );
                accounts.remove_account(account_id);
            }

            _ => {
                println!(
                    "[Regional #{}] Sync: Mensaje no implementado aún",
                    regional_id
                );
            }
        }
    }

    Ok(())
}

/// Obtiene la IP local para reportar al Central
fn get_local_ip() -> io::Result<String> {
    // Para desarrollo, usamos localhost
    // En producción, obtener la IP real de la interfaz
    Ok("127.0.0.1".to_string())
}

/// Crea un payload con framing para el protocolo del Central
fn frame_payload_with_code(code: u8, payload: &[u8]) -> Vec<u8> {
    let packet_size = (payload.len() + 1) as u64; // +1 for code
    let mut buf = Vec::new();
    buf.extend_from_slice(&packet_size.to_be_bytes());
    buf.push(code);
    buf.extend_from_slice(payload);
    buf
}
