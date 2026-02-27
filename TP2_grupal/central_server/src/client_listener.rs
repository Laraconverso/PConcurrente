use crate::database::account_data::AccountData;
use crate::election::messages::StateUpdateData;
use crate::election::ring_election::RingElectionNode;
use crate::regional_registry::RegionalRegistry;
use crate::utils::{AccountDB, TransactionDB};
use chrono::Local;
use rayon::prelude::*;
use shared_resources::database::transaction::{
    Transaction, serialize_transactions, transaction_belongs,
};
use shared_resources::messages::account::register_account::{REG_ACCOUNT, RegisterAccountMessage};
use shared_resources::messages::card::register_card::{REG_CARD, RegisterCardMessage};
use shared_resources::messages::data_response::DataResponseMessage;
use shared_resources::messages::probe::probe_response::{PROBE, ProbeResponse};
use shared_resources::messages::protocol::{
    AccountSyncData, REGIONAL_BATCH, REGISTER_REGIONAL, RegionalBatchMessage,
    RegisterRegionalMessage, RegisterRegionalResponse,
};
use shared_resources::messages::query::query_data::{QUERY, QueryDataMessage};
use shared_resources::messages::remove_regions::{RemoveRegionsMessage, UPD_REMOVE_REGIONS};
use shared_resources::messages::update_data::{
    UPD_ACCOUNT_LIMIT, UPD_ACCOUNT_NAME, UPD_CARD, UpdateDataMessage,
};
use shared_resources::messages::update_regions::{UPD_REGIONS, UpdateRegionsMessage};
use shared_resources::utils::{AccountId, FAILURE, GenId, SUCCESS, TimeStamp, frame_payload};
use std::cmp::max;
use std::error::Error;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Arc, RwLock};
use std::thread;
use std::thread::JoinHandle;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Log informativo con timestamp y componente
fn log_info(node_id: u8, msg: &str) {
    println!(
        "[{}] [CENTRAL #{}] [INFO] {}",
        Local::now().format("%H:%M:%S%.3f"),
        node_id,
        msg
    );
}

/// Log de error con timestamp y componente
fn log_error(node_id: u8, msg: &str) {
    eprintln!(
        "[{}] [CENTRAL #{}] [ERROR] {}",
        Local::now().format("%H:%M:%S%.3f"),
        node_id,
        msg
    );
}

pub struct ClientListener {
    addr: String,
    db: Arc<RwLock<AccountDB>>,
    transaction_db: Arc<RwLock<TransactionDB>>,
    is_leader: Arc<AtomicBool>,
    current_leader_id: Arc<AtomicU8>,
    ring_node: Arc<RingElectionNode>,
    node_id: u8,
    regional_registry: Arc<RegionalRegistry>,
}

impl ClientListener {
    pub fn new(
        addr: &str,
        db: Arc<RwLock<AccountDB>>,
        transaction_db: Arc<RwLock<TransactionDB>>,
        is_leader: Arc<AtomicBool>,
        current_leader_id: Arc<AtomicU8>,
        ring_node: Arc<RingElectionNode>,
        regional_registry: Arc<RegionalRegistry>,
    ) -> Self {
        let node_id = ring_node.node_id;
        Self {
            addr: addr.to_string(),
            db,
            transaction_db,
            is_leader,
            current_leader_id,
            ring_node,
            node_id,
            regional_registry,
        }
    }

    pub fn listen(&self) -> Result<(), Box<dyn Error>> {
        let listener = TcpListener::bind(&self.addr)?;
        let mut handlers: Vec<JoinHandle<()>> = Vec::new();

        for stream in listener.incoming() {
            match stream {
                Ok(stream) => {
                    let db_clone = self.db.clone();
                    let t_clone = self.transaction_db.clone();
                    let is_leader_clone = Arc::clone(&self.is_leader);
                    let current_leader_clone = Arc::clone(&self.current_leader_id);
                    let ring_clone = Arc::clone(&self.ring_node);
                    let registry_clone = Arc::clone(&self.regional_registry);

                    handlers.push(thread::spawn(move || {
                        start_client(
                            stream,
                            db_clone,
                            t_clone,
                            is_leader_clone,
                            current_leader_clone,
                            ring_clone,
                            registry_clone,
                        );
                    }))
                }
                _ => {
                    log_error(self.node_id, "Error while getting stream");
                }
            }
        }

        Ok(())
    }
}

fn start_client(
    mut stream: TcpStream,
    accounts_lock: Arc<RwLock<AccountDB>>,
    transactions_lock: Arc<RwLock<TransactionDB>>,
    is_leader: Arc<AtomicBool>,
    current_leader_id: Arc<AtomicU8>,
    ring_node: Arc<RingElectionNode>,
    regional_registry: Arc<RegionalRegistry>,
) {
    let node_id = ring_node.node_id;
    loop {
        let mut aux_buf = [0u8; 1];
        let n = stream.peek(&mut aux_buf).unwrap_or(0);
        if n == 0 {
            thread::sleep(Duration::from_millis(500));
            continue;
        }

        let mut packet_size_buf = [0u8; 8];
        if stream.read_exact(&mut packet_size_buf).is_err() {
            log_info(node_id, "Cliente desconectado");
            break;
        }
        let packet_size = u64::from_be_bytes(packet_size_buf) as usize;

        let mut code_buf = [0u8; 1];
        if stream.read_exact(&mut code_buf).is_err() {
            break;
        }
        let code = code_buf[0];

        let mut msg_buf = vec![0u8; packet_size - 1];
        if stream.read_exact(&mut msg_buf).is_err() {
            break;
        }

        // PROBE es read-only, no requiere verificación de líder
        if code == PROBE {
            let msg = send_cluster_date(&ring_node);
            let res_bytes = frame_payload(msg.serialize());
            let _ = stream.write_all(&res_bytes);
            let _ = stream.flush();
            continue;
        }

        // VERIFICACIÓN DE LÍDER para TODOS los comandos (excepto PROBE)
        if !is_leader.load(Ordering::SeqCst) {
            let leader_id = current_leader_id.load(Ordering::SeqCst);

            // Obtener la dirección de cliente del líder actual
            let leader_addr = ring_node
                .get_node_client_address(leader_id)
                .unwrap_or_else(|| "127.0.0.1:9000".to_string()); // Fallback

            // REGISTER_REGIONAL usa protocolo especial con RegisterRegionalResponse
            if code == REGISTER_REGIONAL {
                let response = RegisterRegionalResponse::redirect(leader_addr);

                // Crear payload con código + JSON
                let json_bytes = serde_json::to_vec(&response).unwrap();
                let mut payload = Vec::with_capacity(1 + json_bytes.len());
                payload.push(REGISTER_REGIONAL);
                payload.extend_from_slice(&json_bytes);

                // Enmarcar con tamaño total
                let res_bytes = frame_payload(payload);

                if stream.write_all(&res_bytes).is_err() {
                    log_error(node_id, "Error enviando redirección REGISTER_REGIONAL");
                    break;
                }
                if stream.flush().is_err() {
                    break;
                }
                continue;
            }

            // Para otros comandos, usar DataResponseMessage estándar
            let redirect_msg = DataResponseMessage::new(
                code,
                FAILURE,
                leader_id as u64,
                Some(leader_addr.as_bytes().to_vec()),
            );
            let res_bytes = frame_payload(redirect_msg.serialize());
            let _ = stream.write_all(&res_bytes);
            let _ = stream.flush();
            continue;
        }

        // A partir de aquí, garantizamos que somos el líder

        // REGISTER_REGIONAL usa un protocolo simplificado (8 bytes + code + JSON)
        if code == REGISTER_REGIONAL {
            let response =
                handle_register_regional(msg_buf.clone(), &accounts_lock, &regional_registry);

            // Crear payload con código + JSON
            let json_bytes = serde_json::to_vec(&response).unwrap();
            let mut payload = Vec::with_capacity(1 + json_bytes.len());
            payload.push(REGISTER_REGIONAL);
            payload.extend_from_slice(&json_bytes);

            // Enmarcar con tamaño total
            let res_bytes = frame_payload(payload);

            if stream.write_all(&res_bytes).is_err() {
                log_error(node_id, "Error enviando respuesta REGISTER_REGIONAL");
                break;
            }
            if stream.flush().is_err() {
                break;
            }
            continue;
        }

        let msg = execute_command(
            code,
            msg_buf.clone(),
            &accounts_lock,
            &transactions_lock,
            &regional_registry,
            &ring_node,
            node_id,
        );

        if msg.status() == SUCCESS && should_replicate(code) {
            let timestamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs();

            // Obtener el account_id correcto según el tipo de operación
            let account_id = if code == REG_CARD {
                // Para REG_CARD, msg.id() es el card_id, necesitamos extraer account_id del mensaje original
                let card_msg = RegisterCardMessage::deserialize(&msg_buf.clone()).unwrap();
                card_msg.account_id()
            } else {
                // Para otros comandos, msg.id() es el account_id
                msg.id()
            };

            // Serializar la cuenta completa para replicación
            let account_data = {
                let accounts_db = accounts_lock.read().unwrap();
                if let Some(account) = accounts_db.get(&account_id) {
                    // Debug: verificar qué se está serializando
                    if let Ok(json_str) = serde_json::to_string_pretty(account) {
                        eprintln!(
                            "[Replicación] Serializando cuenta {}: {}",
                            account_id, json_str
                        );
                    }
                    serde_json::to_vec(account).unwrap_or_default()
                } else {
                    eprintln!(
                        "[Replicación] ERROR: Cuenta {} no encontrada para serializar",
                        account_id
                    );
                    vec![]
                }
            };

            let state_update = StateUpdateData {
                operation_type: code,
                account_id,
                data: account_data,
                timestamp,
            };
            // Enviar replicación de forma ASÍNCRONA (no bloquear el thread del cliente)
            ring_node.replicate_state_async(state_update);
        }

        let res_bytes = frame_payload(msg.serialize());

        if stream.write_all(&res_bytes).is_err() {
            log_error(node_id, "Error enviando respuesta");
            break;
        }
        if stream.flush().is_err() {
            break;
        }
    }
}

fn execute_command(
    code: u8,
    bytes: Vec<u8>,
    account_lock: &Arc<RwLock<AccountDB>>,
    transactions_lock: &Arc<RwLock<TransactionDB>>,
    regional_registry: &Arc<RegionalRegistry>,
    ring_node: &Arc<RingElectionNode>,
    node_id: u8,
) -> DataResponseMessage {
    match code {
        REG_ACCOUNT => register_account(bytes, account_lock, regional_registry),
        REG_CARD => register_card(bytes, account_lock, regional_registry),
        UPD_ACCOUNT_LIMIT | UPD_ACCOUNT_NAME | UPD_CARD => {
            process_update(bytes, account_lock, regional_registry, code)
        }
        UPD_REGIONS => process_update_regions(bytes, account_lock, regional_registry, node_id),
        UPD_REMOVE_REGIONS => {
            process_remove_regions(bytes, account_lock, regional_registry, node_id)
        }
        QUERY => query_data(bytes, account_lock, transactions_lock),
        PROBE => send_cluster_date(ring_node),
        REGIONAL_BATCH => {
            process_regional_batch(bytes, account_lock, transactions_lock, ring_node, node_id)
        }
        REGISTER_REGIONAL => register_regional(bytes, account_lock, regional_registry),
        _ => unknown_command(code, node_id),
    }
}

/// Genera un account_id sin región (completamente random)
fn generate_account_id() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static ACCOUNT_COUNTER: AtomicU64 = AtomicU64::new(1);

    ACCOUNT_COUNTER.fetch_add(1, Ordering::SeqCst)
}

fn change_limit(
    account: &mut AccountData,
    code: u8,
    new_limit: f64,
    card_id: GenId,
) -> DataResponseMessage {
    let (status, res_id) = account.change_limit(new_limit, card_id);
    DataResponseMessage::new(code, status, res_id, None)
}

fn register_account(
    bytes: Vec<u8>,
    account_lock: &Arc<RwLock<AccountDB>>,
    _regional_registry: &Arc<RegionalRegistry>,
) -> DataResponseMessage {
    let msg = match RegisterAccountMessage::deserialize(&bytes) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("[RegisterAccount] Error deserializando: {}", e);
            return DataResponseMessage::new(REG_ACCOUNT, FAILURE, 0, None);
        }
    };

    // Obtener username y display_name
    let username = msg.username();
    let display_name = msg.display_name();

    // Validar username
    if !is_valid_username(&username) {
        eprintln!("[RegisterAccount] Username inválido: '{}'", username);
        return DataResponseMessage::new(REG_ACCOUNT, FAILURE, 0, None);
    }

    // Generar account_id sin región (simplemente secuencial/random)
    let new_account_id = generate_account_id();

    // Crear cuenta SIN regiones habilitadas
    let data = AccountData::new(username.clone(), display_name.clone(), new_account_id);
    {
        let mut db = account_lock.write().unwrap();
        match db.insert(new_account_id, data) {
            Ok(_) => {}
            Err(e) => {
                eprintln!("[RegisterAccount] {}", e);
                return DataResponseMessage::new(REG_ACCOUNT, FAILURE, 0, None);
            }
        }
    }

    println!(
        "[RegisterAccount] Cuenta '{}' (@{}) creada con ID {} (sin regiones habilitadas)",
        display_name, username, new_account_id
    );

    // NO propagamos a regionales (la cuenta arranca sin regiones)

    DataResponseMessage::new(REG_ACCOUNT, SUCCESS, new_account_id, None)
}

/// Valida que un username sea alfanumérico con guiones y underscores
fn is_valid_username(username: &str) -> bool {
    username.len() >= 3
        && username.len() <= 30
        && username
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
        && !username.starts_with('-')
}

fn register_card(
    bytes: Vec<u8>,
    account_lock: &Arc<RwLock<AccountDB>>,
    regional_registry: &Arc<RegionalRegistry>,
) -> DataResponseMessage {
    let msg = RegisterCardMessage::deserialize(&bytes).unwrap();
    let account_id = msg.account_id();
    let limit = msg.limit();

    let result = {
        let mut account_db = account_lock.write().unwrap();
        if let Some(account) = account_db.get_mut(&account_id) {
            let (new_card_id, status) = account.register_card(limit);
            Some((new_card_id, status))
        } else {
            None
        }
    };

    match result {
        Some((new_card_id, status)) if status == SUCCESS => {
            // Propagar a TODAS las regiones habilitadas de la cuenta
            let enabled_regions = {
                let db = account_lock.read().unwrap();
                if let Some(account) = db.get(&account_id) {
                    account.enabled_regions().clone()
                } else {
                    std::collections::HashMap::new()
                }
            };

            for region_name in enabled_regions.keys() {
                let sync_msg =
                    shared_resources::messages::protocol::CentralSyncMessage::CardUpdate {
                        account_id,
                        card_id: new_card_id,
                        card_limit: limit,
                    };
                regional_registry.propagate(region_name, sync_msg);
                println!(
                    "[RegisterCard] Propagado tarjeta {} a región '{}'",
                    new_card_id, region_name
                );
            }

            DataResponseMessage::new(REG_CARD, status, new_card_id, None)
        }
        Some((new_card_id, status)) => {
            DataResponseMessage::new(REG_CARD, status, new_card_id, None)
        }
        None => DataResponseMessage::new(REG_CARD, FAILURE, 0, None),
    }
}

fn process_update(
    bytes: Vec<u8>,
    account_lock: &Arc<RwLock<AccountDB>>,
    regional_registry: &Arc<RegionalRegistry>,
    code: u8,
) -> DataResponseMessage {
    let msg = UpdateDataMessage::deserialize(&bytes).unwrap();
    let account_id = msg.account_id();

    // Actualizar nombre de cuenta
    let new_name = msg.new_name();
    if !new_name.is_empty() {
        let (active, enabled_regions) = {
            let mut account_db = account_lock.write().unwrap();
            if let Some(account) = account_db.get_mut(&account_id) {
                account.change_name(new_name.clone());
                (account.state(), account.enabled_regions().clone())
            } else {
                return DataResponseMessage::new(code, FAILURE, account_id, None);
            }
        };

        // Propagar a TODAS las regionales habilitadas
        {
            let accounts = account_lock.read().unwrap();
            let account_username = accounts
                .get(&account_id)
                .map(|a| a.username())
                .unwrap_or_default();
            drop(accounts);

            // Propagar a cada región habilitada
            for (region_name, region_limit) in &enabled_regions {
                let sync_msg =
                    shared_resources::messages::protocol::CentralSyncMessage::AccountUpdate {
                        account_id,
                        username: account_username.clone(),
                        display_name: new_name.clone(),
                        region_name: region_name.clone(),
                        region_limit: *region_limit,
                        active,
                    };
                regional_registry.propagate(region_name, sync_msg);
            }
        }

        return DataResponseMessage::new(code, SUCCESS, account_id, None);
    }

    // Actualizar límite (cuenta o tarjeta)
    let card_id = msg.card_id();
    let new_limit = msg.limit();

    let response = {
        let mut account_db = account_lock.write().unwrap();
        if let Some(account) = account_db.get_mut(&account_id) {
            change_limit(account, code, new_limit, card_id)
        } else {
            DataResponseMessage::new(code, FAILURE, 0, None)
        }
    };

    // Si fue exitoso, propagar al Regional
    if response.status() == SUCCESS {
        if card_id == 0 {
            // TODO Multi-Regional: Actualización de límite de cuenta por región
            // Esto requiere que el comando UPD -r esté implementado con la sintaxis:
            // UPD -r <región1:límite1> [región2:límite2] ...
            // Por ahora, no se propaga automáticamente (se implementará en UPD -r)
            println!(
                "[Central] WARN: Actualización de límite de cuenta no se propaga (usar UPD -r cuando esté implementado)"
            );
        } else {
            // Actualización de límite de tarjeta: propagar a TODAS las regiones habilitadas
            let enabled_regions = {
                let db = account_lock.read().unwrap();
                let account = db.get(&account_id).unwrap();
                account.enabled_regions().clone()
            };

            for region_name in enabled_regions.keys() {
                let sync_msg =
                    shared_resources::messages::protocol::CentralSyncMessage::CardUpdate {
                        account_id,
                        card_id: response.id(), // La respuesta contiene el card_id
                        card_limit: new_limit,
                    };
                regional_registry.propagate(region_name, sync_msg);
            }
        }
    }

    response
}

fn process_update_regions(
    bytes: Vec<u8>,
    account_lock: &Arc<RwLock<AccountDB>>,
    regional_registry: &Arc<RegionalRegistry>,
    node_id: u8,
) -> DataResponseMessage {
    log_info(
        node_id,
        &format!(
            "[UpdateRegions] DEBUG: Recibidos {} bytes: {:?}",
            bytes.len(),
            &bytes[..std::cmp::min(20, bytes.len())]
        ),
    );
    let msg = match UpdateRegionsMessage::deserialize(&bytes) {
        Ok(m) => m,
        Err(e) => {
            log_error(
                node_id,
                &format!("Error deserializando UpdateRegionsMessage: {}", e),
            );
            return DataResponseMessage::new(UPD_REGIONS, FAILURE, 0, None);
        }
    };

    let account_id = msg.account_id();
    let regions = msg.regions();
    let limits = msg.limits();

    log_info(
        node_id,
        &format!(
            "[UpdateRegions] Cuenta {}: agregando {} regiones",
            account_id,
            regions.len()
        ),
    );

    // Validar que todas las regiones existen
    for region_name in regions {
        if regional_registry.get_region_id(region_name).is_none() {
            log_error(
                node_id,
                &format!("[UpdateRegions] Región '{}' no encontrada", region_name),
            );
            let available_regions = regional_registry.list_regions();
            log_info(
                node_id,
                &format!(
                    "[UpdateRegions] Regiones disponibles: {:?}",
                    available_regions
                ),
            );

            // Serializar la lista de regiones disponibles en el piggyback
            let error_msg = format!(
                "Región '{}' no existe. Regiones disponibles: {}",
                region_name,
                available_regions.join(", ")
            );
            return DataResponseMessage::new(
                UPD_REGIONS,
                FAILURE,
                account_id,
                Some(error_msg.as_bytes().to_vec()),
            );
        }
    }

    // Actualizar las regiones en la cuenta
    {
        let mut account_db = account_lock.write().unwrap();
        if let Some(account) = account_db.get_mut(&account_id) {
            for i in 0..regions.len() {
                account.add_region(regions[i].clone(), limits[i]);
                log_info(
                    node_id,
                    &format!(
                        "[UpdateRegions] Región '{}' agregada con límite {:.2}",
                        regions[i], limits[i]
                    ),
                );
            }
        } else {
            log_error(
                node_id,
                &format!("[UpdateRegions] Cuenta {} no encontrada", account_id),
            );
            return DataResponseMessage::new(UPD_REGIONS, FAILURE, account_id, None);
        }
    }

    // Propagar a cada región afectada
    let (username, display_name, active, cards) = {
        let db = account_lock.read().unwrap();
        let account = db.get(&account_id).unwrap();
        (
            account.username(),
            account.display_name(),
            account.state(),
            account.cards(),
        )
    };

    for i in 0..regions.len() {
        // Propagar la cuenta
        let sync_msg = shared_resources::messages::protocol::CentralSyncMessage::AccountUpdate {
            account_id,
            username: username.clone(),
            display_name: display_name.clone(),
            region_name: regions[i].clone(),
            region_limit: limits[i],
            active,
        };
        regional_registry.propagate(&regions[i], sync_msg);
        log_info(
            node_id,
            &format!("[UpdateRegions] Propagado a región '{}'", regions[i]),
        );

        // Propagar todas las tarjetas existentes de la cuenta
        for (card_id, card_limit) in &cards {
            let card_sync_msg =
                shared_resources::messages::protocol::CentralSyncMessage::CardUpdate {
                    account_id,
                    card_id: *card_id,
                    card_limit: *card_limit,
                };
            regional_registry.propagate(&regions[i], card_sync_msg);
            log_info(
                node_id,
                &format!(
                    "[UpdateRegions] Propagada tarjeta {} a región '{}'",
                    card_id, regions[i]
                ),
            );
        }
    }

    log_info(
        node_id,
        &format!(
            "[UpdateRegions] Regiones actualizadas para cuenta {} (con {} tarjetas)",
            account_id,
            cards.len()
        ),
    );
    DataResponseMessage::new(UPD_REGIONS, SUCCESS, account_id, None)
}

fn process_remove_regions(
    bytes: Vec<u8>,
    account_lock: &Arc<RwLock<AccountDB>>,
    regional_registry: &Arc<RegionalRegistry>,
    node_id: u8,
) -> DataResponseMessage {
    let msg = match RemoveRegionsMessage::deserialize(&bytes) {
        Ok(m) => m,
        Err(e) => {
            log_error(
                node_id,
                &format!("Error deserializando RemoveRegionsMessage: {}", e),
            );
            return DataResponseMessage::new(UPD_REMOVE_REGIONS, FAILURE, 0, None);
        }
    };

    let account_id = msg.account_id();
    let regions = msg.regions();

    log_info(
        node_id,
        &format!(
            "[RemoveRegions] Cuenta {}: removiendo {} regiones",
            account_id,
            regions.len()
        ),
    );

    // Validar y remover las regiones de la cuenta
    let (successfully_removed, not_found) = {
        let mut account_db = account_lock.write().unwrap();
        if let Some(account) = account_db.get_mut(&account_id) {
            let mut removed = Vec::new();
            let mut not_found_list = Vec::new();

            for region_name in regions {
                if account.remove_region(region_name) {
                    removed.push(region_name.clone());
                    log_info(
                        node_id,
                        &format!("[RemoveRegions] Región '{}' removida", region_name),
                    );
                } else {
                    not_found_list.push(region_name.clone());
                    log_error(
                        node_id,
                        &format!(
                            "[RemoveRegions] Región '{}' no está asociada a la cuenta",
                            region_name
                        ),
                    );
                }
            }

            (removed, not_found_list)
        } else {
            log_error(
                node_id,
                &format!("[RemoveRegions] Cuenta {} no encontrada", account_id),
            );
            return DataResponseMessage::new(UPD_REMOVE_REGIONS, FAILURE, account_id, None);
        }
    };

    // Si no se removió ninguna región, retornar error
    if successfully_removed.is_empty() {
        let error_msg = format!(
            "No se removieron regiones. Regiones no encontradas: {:?}",
            not_found
        );
        log_error(node_id, &format!("[RemoveRegions] {}", error_msg));
        return DataResponseMessage::new(
            UPD_REMOVE_REGIONS,
            FAILURE,
            account_id,
            Some(error_msg.as_bytes().to_vec()),
        );
    }

    // Enviar RemoveAccount a cada regional afectado
    for region_name in &successfully_removed {
        let sync_msg = shared_resources::messages::protocol::CentralSyncMessage::RemoveAccount {
            account_id,
            region_name: region_name.clone(),
        };
        regional_registry.propagate(region_name, sync_msg);
        log_info(
            node_id,
            &format!(
                "[RemoveRegions] Enviado RemoveAccount a región '{}'",
                region_name
            ),
        );
    }

    // Log de resumen
    if !not_found.is_empty() {
        log_info(
            node_id,
            &format!(
                "[RemoveRegions] ADVERTENCIA: Algunas regiones no se encontraron: {:?}",
                not_found
            ),
        );
    }
    log_info(
        node_id,
        &format!(
            "[RemoveRegions] {} regiones removidas correctamente",
            successfully_removed.len()
        ),
    );
    DataResponseMessage::new(UPD_REMOVE_REGIONS, SUCCESS, account_id, None)
}

fn query_data(
    bytes: Vec<u8>,
    account_lock: &Arc<RwLock<AccountDB>>,
    transactions_lock: &Arc<RwLock<TransactionDB>>,
) -> DataResponseMessage {
    let msg = QueryDataMessage::deserialize(&bytes).unwrap();
    // Si tengo card id, se filtra por tarjeta.
    // Si tengo fecha, filtro por fechas también. TODO: Ver si es necesario el filtrado.
    let account_id = msg.account_id();
    if msg.query_type() == 1 {
        return query_account_data(account_lock, account_id, transactions_lock);
    }
    let card_id = msg.card_id();

    let red_id = if card_id == 0 { account_id } else { card_id };

    let lower_bound = max(0, msg.start_date());
    let upper_bound = if msg.end_date() == 0 {
        TimeStamp::MAX as TimeStamp
    } else {
        msg.end_date()
    };

    let transactions_db = transactions_lock.read().unwrap();
    if !transactions_db.contains_key(&account_id) {
        return DataResponseMessage::new(QUERY, FAILURE, red_id, None);
    }

    let transactions = transactions_db.get(&account_id).unwrap();
    let filtered: Vec<Transaction> = transactions
        .par_iter()
        .filter(|x| {
            if transaction_belongs(lower_bound, upper_bound, x.date()) {
                return match card_id {
                    0 => true, // Depende solo de la fecha
                    _ => card_id == x.card_id(),
                };
            }
            false
        })
        .cloned()
        .collect();
    let piggybacked = serialize_transactions(filtered);
    DataResponseMessage::new(QUERY, SUCCESS, red_id, Some(piggybacked))
}

fn unknown_command(code: u8, node_id: u8) -> DataResponseMessage {
    log_error(node_id, &format!("Código de comando desconocido: {}", code));
    DataResponseMessage::new(code, FAILURE, 0, None)
}

/// Registra un Regional Server (o recupera su info si ya existe)
/// Maneja el registro de un regional server y devuelve la respuesta directamente
fn handle_register_regional(
    bytes: Vec<u8>,
    account_lock: &Arc<RwLock<AccountDB>>,
    regional_registry: &Arc<RegionalRegistry>,
) -> RegisterRegionalResponse {
    let msg: RegisterRegionalMessage = match serde_json::from_slice(&bytes) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("[RegisterRegional] Error deserializando: {}", e);
            return RegisterRegionalResponse::error(
                format!("Error deserializando mensaje: {}", e),
                regional_registry.list_regions(),
            );
        }
    };

    let region_name = msg.name.clone();
    let sync_address = msg.sync_address.clone();

    println!(
        "[RegisterRegional] Procesando registro de '{}'",
        region_name
    );

    match regional_registry.register(msg.name, msg.sync_address) {
        Ok((region_id, is_new)) => {
            if is_new {
                println!(
                    "[RegisterRegional] Nuevo regional '{}' registrado con ID {} en {}",
                    region_name, region_id, sync_address
                );
            } else {
                println!(
                    "[RegisterRegional] Regional '{}' re-registrado (ID {} en {})",
                    region_name, region_id, sync_address
                );
            }

            let accounts = get_accounts_for_region(&region_name, account_lock);

            println!(
                "[RegisterRegional] Enviando {} cuentas a '{}'",
                accounts.len(),
                region_name
            );

            RegisterRegionalResponse::success(region_id, accounts, regional_registry.list_regions())
        }
        Err(e) => {
            eprintln!(
                "[RegisterRegional] Error registrando '{}': {}",
                region_name, e
            );
            RegisterRegionalResponse::error(
                format!("Error: {}", e),
                regional_registry.list_regions(),
            )
        }
    }
}

/// Versión legacy que devuelve DataResponseMessage (mantener para compatibilidad)
fn register_regional(
    bytes: Vec<u8>,
    account_lock: &Arc<RwLock<AccountDB>>,
    regional_registry: &Arc<RegionalRegistry>,
) -> DataResponseMessage {
    let msg: RegisterRegionalMessage = match serde_json::from_slice(&bytes) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("[RegisterRegional] Error deserializando: {}", e);
            let response = RegisterRegionalResponse::error(
                format!("Error deserializando mensaje: {}", e),
                regional_registry.list_regions(),
            );
            return DataResponseMessage::new(
                REGISTER_REGIONAL,
                FAILURE,
                0,
                Some(serde_json::to_vec(&response).unwrap()),
            );
        }
    };

    let region_name = msg.name;
    let sync_address = msg.sync_address;

    // Registrar el regional
    match regional_registry.register(region_name.clone(), sync_address.clone()) {
        Ok((region_id, is_new)) => {
            if is_new {
                println!(
                    "[RegisterRegional] Nuevo regional '{}' registrado con ID {} en {}",
                    region_name, region_id, sync_address
                );
            } else {
                println!(
                    "[RegisterRegional] Regional '{}' reconectado (ID {})",
                    region_name, region_id
                );
            }

            // Obtener todas las cuentas de esta región para enviarlas al Regional
            let accounts = get_accounts_for_region(&region_name, account_lock);

            println!(
                "[RegisterRegional] Enviando {} cuentas a '{}'",
                accounts.len(),
                region_name
            );

            let response = RegisterRegionalResponse::success(
                region_id,
                accounts,
                regional_registry.list_regions(),
            );

            DataResponseMessage::new(
                REGISTER_REGIONAL,
                SUCCESS,
                region_id as u64,
                Some(serde_json::to_vec(&response).unwrap()),
            )
        }
        Err(e) => {
            eprintln!(
                "[RegisterRegional] Error registrando '{}': {}",
                region_name, e
            );
            let response = RegisterRegionalResponse::error(
                format!("Error: {}", e),
                regional_registry.list_regions(),
            );
            DataResponseMessage::new(
                REGISTER_REGIONAL,
                FAILURE,
                0,
                Some(serde_json::to_vec(&response).unwrap()),
            )
        }
    }
}

/// Obtiene todas las cuentas habilitadas en una región específica
fn get_accounts_for_region(
    region_name: &str,
    account_lock: &Arc<RwLock<AccountDB>>,
) -> Vec<AccountSyncData> {
    let accounts_db = account_lock.read().unwrap();

    accounts_db
        .iter()
        .filter_map(|(account_id, account_data)| {
            // Filtrar solo las cuentas que tienen esta región habilitada
            account_data
                .get_region_limit(region_name)
                .map(|region_limit| AccountSyncData {
                    account_id: *account_id,
                    username: account_data.username(),
                    display_name: account_data.display_name(),
                    limit: region_limit, // Límite específico de esta región
                    active: account_data.state(),
                    cards: account_data.cards().into_iter().collect(),
                })
        })
        .collect()
}

/// Determina si una operación debe ser replicada a las otras réplicas.
/// Solo las operaciones de escritura necesitan replicación.
fn should_replicate(code: u8) -> bool {
    matches!(
        code,
        REG_ACCOUNT |        // Registro de nueva cuenta
        REG_CARD |           // Registro de nueva tarjeta
        UPD_ACCOUNT_LIMIT |  // Actualización de límite de cuenta
        UPD_CARD |           // Actualización de límite de tarjeta
        UPD_ACCOUNT_NAME |   // Actualización de nombre
        UPD_REGIONS |        // Actualización de regiones
        UPD_REMOVE_REGIONS // Remoción de regiones
    )
}

fn query_account_data(
    accounts_lock: &Arc<RwLock<AccountDB>>,
    account_id: AccountId,
    transactions_lock: &Arc<RwLock<TransactionDB>>,
) -> DataResponseMessage {
    let accounts_db = accounts_lock.read().unwrap();
    println!(
        "[QueryAccountData] DEBUG: Buscando cuenta con ID {}",
        account_id
    );
    if !accounts_db.contains_key(&account_id) {
        println!(
            "[QueryAccountData] ERROR: Cuenta {} no encontrada",
            account_id
        );
        return DataResponseMessage::new(QUERY, FAILURE, account_id, None);
    }
    println!("[QueryAccountData] Cuenta {} encontrada", account_id);

    let account = accounts_db.get(&account_id).unwrap();
    let username = account.username();
    let display_name = account.display_name();
    let enabled_regions = account.enabled_regions().clone(); // HashMap<String, f64>
    let cards = account.cards();
    drop(accounts_db);

    // Calcular gastos totales de la cuenta y por tarjeta
    let transactions_db = transactions_lock.read().unwrap();
    let mut total_spent = 0.0;
    let mut card_spending: std::collections::HashMap<u64, f64> = std::collections::HashMap::new();

    if let Some(transactions) = transactions_db.get(&account_id) {
        for tx in transactions {
            total_spent += tx.amount();
            *card_spending.entry(tx.card_id()).or_insert(0.0) += tx.amount();
        }
    }
    drop(transactions_db);

    let mut piggybacked = Vec::new();

    piggybacked.push(1);

    // Username
    let username_bytes = username.as_bytes();
    let username_len = username_bytes.len() as u64;
    piggybacked.extend_from_slice(&username_len.to_be_bytes());
    piggybacked.extend_from_slice(username_bytes);

    // Display name
    let display_bytes = display_name.as_bytes();
    let display_len = display_bytes.len() as u64;
    piggybacked.extend_from_slice(&display_len.to_be_bytes());
    piggybacked.extend_from_slice(display_bytes);

    // Serializar regiones habilitadas: [num_regions, region1_name_len, region1_name, region1_limit, ...]
    piggybacked.extend_from_slice(&(enabled_regions.len() as u64).to_be_bytes());
    for (region_name, region_limit) in &enabled_regions {
        let region_bytes = region_name.as_bytes();
        let region_len = region_bytes.len() as u64;
        piggybacked.extend_from_slice(&region_len.to_be_bytes());
        piggybacked.extend_from_slice(region_bytes);
        piggybacked.extend_from_slice(&region_limit.to_be_bytes());
    }

    piggybacked.extend_from_slice(&total_spent.to_be_bytes());

    piggybacked.extend_from_slice(&(cards.len() as u64).to_be_bytes());
    for (card_id, c_limit) in &cards {
        let card_spent = card_spending.get(card_id).unwrap_or(&0.0);
        piggybacked.extend_from_slice(&card_id.to_be_bytes());
        piggybacked.extend_from_slice(&c_limit.to_be_bytes());
        piggybacked.extend_from_slice(&card_spent.to_be_bytes()); // Nuevo: gasto por tarjeta
    }

    DataResponseMessage::new(QUERY, SUCCESS, account_id, Some(piggybacked))
}

fn send_cluster_date(ring_node: &Arc<RingElectionNode>) -> DataResponseMessage {
    let addrs = ring_node.get_cluster_client_addrs();
    let l_addr = ring_node.get_leader_addr().unwrap();
    let msg = ProbeResponse::new(l_addr, addrs);

    let piggybacked = msg.serialize();

    DataResponseMessage::new(PROBE, SUCCESS, 0, Some(piggybacked))
}

fn process_regional_batch(
    bytes: Vec<u8>,
    account_lock: &Arc<RwLock<AccountDB>>,
    transactions_lock: &Arc<RwLock<TransactionDB>>,
    ring_node: &Arc<RingElectionNode>,
    node_id: u8,
) -> DataResponseMessage {
    // Deserializar el batch del regional
    let batch: RegionalBatchMessage = match serde_json::from_slice(&bytes) {
        Ok(b) => b,
        Err(e) => {
            log_error(
                node_id,
                &format!("RegionalBatch - Error deserializando batch: {}", e),
            );
            return DataResponseMessage::new(REGIONAL_BATCH, FAILURE, 0, None);
        }
    };

    log_info(
        node_id,
        &format!(
            "RegionalBatch - Procesando lote del Regional #{} con {} transacciones",
            batch.regional_id,
            batch.transactions.len()
        ),
    );

    // Procesar cada transacción
    let mut success_count = 0;
    let mut failure_count = 0;

    for tx in batch.transactions {
        // Verificar si la cuenta existe
        let mut accounts_db = account_lock.write().unwrap();
        if let Some(account) = accounts_db.get_mut(&tx.account_id) {
            // Validar límites y procesar transacción
            let card_limit = account.cards().get(&tx.card_id).copied();

            if let Some(limit) = card_limit {
                if tx.amount <= limit {
                    // Transacción válida - registrar
                    drop(accounts_db); // Soltar lock antes de obtener el siguiente

                    let mut txn_db = transactions_lock.write().unwrap();
                    // Constructor: new(date, card_id, gas_station_id, pump_number, amount, fees)
                    // Nota: No tenemos pump_number en TransactionMessage, usamos 0 como default
                    let transaction = Transaction::new(
                        tx.timestamp as i64, // date (TimeStamp)
                        tx.card_id,          // card_id
                        tx.station_id,       // gas_station_id
                        0,                   // pump_number (no disponible en el mensaje)
                        tx.amount,           // amount
                        0.0,                 // fees (por ahora 0)
                    );

                    // Agregar la transacción al vector de transacciones de la cuenta
                    txn_db
                        .entry(tx.account_id)
                        .or_default()
                        .push(transaction.clone());
                    drop(txn_db);

                    // Replicar la transacción a otros nodos
                    if should_replicate(REGIONAL_BATCH) {
                        let tx_data = serde_json::to_vec(&transaction).unwrap_or_default();
                        ring_node.replicate_state_async(StateUpdateData {
                            operation_type: 254, // Código para transacción individual
                            account_id: tx.account_id,
                            data: tx_data,
                            timestamp: tx.timestamp,
                        });
                    }

                    success_count += 1;
                    log_info(
                        node_id,
                        &format!(
                            "RegionalBatch - TX {} procesada: cuenta={}, monto={:.2}",
                            tx.transaction_id, tx.account_id, tx.amount
                        ),
                    );
                } else {
                    failure_count += 1;
                    log_error(
                        node_id,
                        &format!(
                            "RegionalBatch - TX rechazada: monto {:.2} excede límite {:.2}",
                            tx.amount, limit
                        ),
                    );
                }
            } else {
                failure_count += 1;
                log_error(
                    node_id,
                    &format!(
                        "RegionalBatch - TX rechazada: tarjeta {} no encontrada",
                        tx.card_id
                    ),
                );
            }
        } else {
            drop(accounts_db);
            failure_count += 1;
            log_error(
                node_id,
                &format!(
                    "RegionalBatch - TX rechazada: cuenta {} no encontrada",
                    tx.account_id
                ),
            );
        }
    }

    log_info(
        node_id,
        &format!(
            "RegionalBatch - Resumen: {} exitosas, {} fallidas",
            success_count, failure_count
        ),
    );

    // Responder con éxito (el batch fue procesado, aunque algunas transacciones puedan haber fallado)
    DataResponseMessage::new(REGIONAL_BATCH, SUCCESS, success_count, None)
}
