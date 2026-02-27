mod client_listener;
pub mod database;
mod election;
mod regional_registry;
mod utils;

use crate::client_listener::ClientListener;
use crate::database::account_data::AccountData;
use crate::election::NodeInfo;
use crate::election::messages::StateUpdateData;
use crate::election::ring_election::RingElectionNode;
use crate::regional_registry::RegionalRegistry;
use crate::utils::{AccountDB, TransactionDB};
use chrono::Local;
use shared_resources::database::transaction::Transaction;
use shared_resources::utils::AccountId;
use std::collections::HashMap;
use std::process;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, RwLock};
use std::{env, thread};

pub static REGION_COUNT: AtomicI64 = AtomicI64::new(0);

/// Log informativo con timestamp y componente
fn log_info(node_id: u8, msg: &str) {
    println!(
        "[{}] [CENTRAL #{}] [INFO] {}",
        Local::now().format("%H:%M:%S%.3f"),
        node_id,
        msg
    );
}

fn main() {
    let args: Vec<String> = env::args().collect();

    // Argumentos: programa --id <node_id> --ring-port <port> --successor <ip:port> [--client-port <port>]
    if args.len() < 7 {
        eprintln!(
            "Uso: {} --id <node_id> --ring-port <port> --successor <ip:port> [--client-port <port>]",
            args[0]
        );
        eprintln!(
            "Ejemplo: {} --id 1 --ring-port 7001 --successor 127.0.0.1:7002 --client-port 6789",
            args[0]
        );
        eprintln!("Nota: Los servidores regionales se registran dinámicamente al arrancar");
        process::exit(1);
    }

    let node_id = parse_arg(&args, "--id").expect("Falta --id");
    let ring_port = parse_arg(&args, "--ring-port").expect("Falta --ring-port");
    let successor_addr = parse_arg_string(&args, "--successor").expect("Falta --successor");
    let client_port: u16 = parse_arg(&args, "--client-port").unwrap_or(6789);

    log_info(node_id, "Configuración:");
    log_info(node_id, &format!("   - Node ID:          {}", node_id));
    log_info(node_id, &format!("   - Puerto Clientes:  {}", client_port));
    log_info(node_id, &format!("   - Puerto Anillo:    {}", ring_port));
    log_info(
        node_id,
        &format!("   - Sucesor:          {}", successor_addr),
    );

    let info_db: Arc<RwLock<AccountDB>> = Arc::new(RwLock::new(AccountDB::new()));
    let transactions_db: Arc<RwLock<TransactionDB>> = Arc::new(RwLock::new(HashMap::new()));

    // Crear registro de regionales (se registran dinámicamente)
    let regional_registry = Arc::new(RegionalRegistry::new());

    populate_test_data(&info_db, &transactions_db);

    let (state_tx, state_rx) = mpsc::channel::<StateUpdateData>();

    let cluster_nodes = vec![NodeInfo {
        id: node_id,
        addr: format!("127.0.0.1:{}", ring_port),
        client_addr: format!("127.0.0.1:{}", client_port),
    }];

    // Clonar para el thread de replicación ANTES de moverlos al ring
    let replication_successor = successor_addr.clone();
    let replication_cluster = cluster_nodes.clone();

    // Clonar state_tx para el snapshot thread ANTES de moverlo al ring
    let snapshot_tx = state_tx.clone();

    let ring_node = Arc::new(RingElectionNode::new(
        node_id,
        ring_port,
        successor_addr,
        0,
        Some(state_tx),
        cluster_nodes,
        format!("127.0.0.1:{}", client_port),
    ));

    Arc::clone(&ring_node).start();
    thread::sleep(std::time::Duration::from_secs(2));

    // Si somos líder, enviar snapshot completo periódicamente
    // Usa el mismo canal (state_tx) que el Thread 3 para propagación
    // Esto mantiene la separación completa del ring
    if ring_node.is_leader.load(Ordering::SeqCst) {
        let snapshot_db = Arc::clone(&info_db);
        let snapshot_txn_db = Arc::clone(&transactions_db);
        let snapshot_is_leader = Arc::clone(&ring_node.is_leader);
        let snapshot_sender = snapshot_tx; // Reutilizar el canal del Thread 3
        let snapshot_node_id = node_id;

        thread::spawn(move || {
            loop {
                thread::sleep(std::time::Duration::from_secs(5)); // Snapshot cada 5s

                // Solo si seguimos siendo líder
                if !snapshot_is_leader.load(Ordering::SeqCst) {
                    break;
                }

                // Serializar todas las cuentas
                let accounts_data = {
                    let db = snapshot_db.read().unwrap();
                    let accounts: Vec<&AccountData> = db.iter().map(|(_, acc)| acc).collect();
                    if accounts.is_empty() {
                        continue; // No hay nada que enviar
                    }
                    serde_json::to_vec(&accounts).unwrap_or_default()
                };

                if !accounts_data.is_empty() {
                    // Enviar snapshot de cuentas al canal - Thread 3 lo propagará por el anillo
                    let snapshot_update = StateUpdateData {
                        operation_type: 255, // Código especial para full sync de cuentas
                        account_id: 0,
                        data: accounts_data,
                        timestamp: std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap()
                            .as_secs(),
                    };

                    if snapshot_sender.send(snapshot_update).is_err() {
                        eprintln!(
                            "[Snapshot #{}] [ERROR] Failed to send accounts snapshot to channel",
                            snapshot_node_id
                        );
                        break; // Canal cerrado, terminar thread
                    }
                }

                // Serializar todas las transacciones
                let txns_data = {
                    let db = snapshot_txn_db.read().unwrap();
                    if db.is_empty() {
                        Vec::new()
                    } else {
                        serde_json::to_vec(&*db).unwrap_or_default()
                    }
                };

                if !txns_data.is_empty() {
                    // Enviar snapshot de transacciones al canal
                    let txn_snapshot_update = StateUpdateData {
                        operation_type: 253, // Código especial para full sync de transacciones
                        account_id: 0,
                        data: txns_data,
                        timestamp: std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap()
                            .as_secs(),
                    };

                    if snapshot_sender.send(txn_snapshot_update).is_err() {
                        eprintln!(
                            "[Snapshot #{}] [ERROR] Failed to send transactions snapshot to channel",
                            snapshot_node_id
                        );
                        break;
                    }

                    eprintln!(
                        "[Snapshot #{}] [INFO] Full snapshot (accounts + transactions) sent to Thread 3",
                        snapshot_node_id
                    );
                }
            }
        });
    }

    // Thread dedicado a replicación (consume del canal y aplica/propaga según el rol)
    // COMPLETAMENTE INDEPENDIENTE del ring - usa sus propias conexiones TCP
    let replication_db = Arc::clone(&info_db);
    let replication_txn_db = Arc::clone(&transactions_db);
    let _replication_is_leader = Arc::clone(&ring_node.is_leader);
    let replication_node_id = node_id;

    thread::spawn(move || {
        use std::io::Write;
        use std::net::TcpStream;
        use std::time::Duration;

        // NO usar cache de conexiones - handle_ring_connection cierra después de cada mensaje

        for update in state_rx {
            // 1. Aplicar cambio localmente en TODOS los nodos (líder y followers)
            if let Err(e) = apply_state_update(update.clone(), &replication_db, &replication_txn_db)
            {
                eprintln!(
                    "[{}] [CENTRAL #{}] [ERROR] Error aplicando cambio localmente: {}",
                    shared_resources::utils::get_timestamp(),
                    replication_node_id,
                    e
                );
                continue;
            }

            // 2. Propagar directamente al sucesor (TODOS los nodos, no solo el líder)
            // Este thread tiene sus PROPIAS conexiones TCP, totalmente separado del ring
            // Los updates circulan: Líder -> N2 -> N3 -> ... -> Líder (se descarta si vuelve)
            {
                // Serializar el mensaje StateUpdate
                let msg = election::messages::ElectionMessage::StateUpdate {
                    operation_type: update.operation_type,
                    account_id: update.account_id,
                    data: update.data,
                    timestamp: update.timestamp,
                };

                let json_msg = msg.to_json();
                let msg_with_newline = format!("{}\n", json_msg);

                // Crear nueva conexión para cada mensaje (no cachear)
                let mut send_successful = false;

                // Intentar con el sucesor primario
                {
                    eprintln!(
                        "[Thread3 #{}] [INFO] Creating new connection to {}",
                        replication_node_id, replication_successor
                    );
                    // Intentar con el sucesor primario
                    if let Ok(mut new_stream) = TcpStream::connect_timeout(
                        &replication_successor.parse().unwrap(),
                        Duration::from_millis(500), // Timeout más generoso para evitar falsos positivos
                    ) {
                        let _ = new_stream.set_write_timeout(Some(Duration::from_millis(500)));
                        if new_stream.write_all(msg_with_newline.as_bytes()).is_ok()
                            && new_stream.flush().is_ok()
                        {
                            eprintln!(
                                "[Thread3 #{}] [INFO] Sent via new connection",
                                replication_node_id
                            );
                            // NO cachear - cada mensaje usa su propia conexión
                            // porque handle_ring_connection cierra después de un mensaje
                            send_successful = true;
                        } else {
                            eprintln!(
                                "[Thread3 #{}] [ERROR] Failed writing to new connection",
                                replication_node_id
                            );
                        }
                    } else {
                        eprintln!(
                            "[Thread3 #{}] [ERROR] Timeout connecting to {}",
                            replication_node_id, replication_successor
                        );
                    }
                }

                // Si falló con el primario, intentar con UN alternativo
                if !send_successful && !replication_cluster.is_empty() {
                    eprintln!(
                        "[Thread3 #{}] [WARN] Primary failed, trying alternative",
                        replication_node_id
                    );
                    // Intentar con el primer nodo alternativo
                    if let Some(node) = replication_cluster.first() {
                        if let Ok(mut alt_stream) = TcpStream::connect_timeout(
                            &node.addr.parse().unwrap(),
                            Duration::from_millis(500),
                        ) {
                            let _ = alt_stream.set_write_timeout(Some(Duration::from_millis(500)));
                            if alt_stream.write_all(msg_with_newline.as_bytes()).is_ok()
                                && alt_stream.flush().is_ok()
                            {
                                eprintln!(
                                    "[Thread3 #{}] [INFO] Sent via alternative {}",
                                    replication_node_id, node.addr
                                );
                                send_successful = true;
                            } else {
                                eprintln!(
                                    "[Thread3 #{}] [ERROR] Failed writing to alternative",
                                    replication_node_id
                                );
                            }
                        } else {
                            eprintln!(
                                "[Thread3 #{}] [ERROR] Timeout connecting to alternative",
                                replication_node_id
                            );
                        }
                    }
                }

                if !send_successful {
                    eprintln!(
                        "[Thread3 #{}] [WARN] Could not propagate update (best-effort)",
                        replication_node_id
                    );
                }
            }
        }
    });

    let client_db = Arc::clone(&info_db);
    let client_txn = Arc::clone(&transactions_db);
    let is_leader = Arc::clone(&ring_node.is_leader);
    let current_leader = Arc::clone(&ring_node.current_leader_id);
    let ring_node_for_client = Arc::clone(&ring_node);

    let regional_registry_for_client = Arc::clone(&regional_registry);

    thread::spawn(move || {
        let addr = format!("127.0.0.1:{}", client_port);
        let client_listener = ClientListener::new(
            &addr,
            client_db,
            client_txn,
            is_leader,
            current_leader,
            ring_node_for_client,
            regional_registry_for_client,
        );
        client_listener.listen().expect("Error while listening");
    });

    log_info(node_id, "Servidor iniciado");

    thread::park();
}

/// Parsea un argumento numérico de la línea de comandos.
fn parse_arg<T: std::str::FromStr>(args: &[String], flag: &str) -> Option<T> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1))
        .and_then(|s| s.parse().ok())
}

/// Parsea un argumento de string de la línea de comandos.
fn parse_arg_string(args: &[String], flag: &str) -> Option<String> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

fn apply_state_update(
    update: StateUpdateData,
    info_db: &Arc<RwLock<AccountDB>>,
    transactions_db: &Arc<RwLock<TransactionDB>>,
) -> Result<(), String> {
    // Caso especial: operation_type 255 = Full Sync de cuentas (snapshot completo)
    if update.operation_type == 255 {
        if update.data.is_empty() {
            return Ok(()); // Snapshot vacío
        }

        eprintln!("[apply_state_update] [INFO] Applying full accounts snapshot");
        let accounts_vec: Vec<AccountData> = serde_json::from_slice(&update.data)
            .map_err(|e| format!("Error deserializando snapshot: {}", e))?;

        let mut accounts_db = info_db.write().unwrap();
        for account in accounts_vec {
            let account_id = account.id();
            accounts_db.upsert(account_id, account);
        }
        eprintln!(
            "[apply_state_update] [INFO] Full accounts snapshot applied - {} accounts",
            accounts_db.iter().count()
        );
        return Ok(());
    }

    // Caso especial: operation_type 254 = Transacción individual
    if update.operation_type == 254 {
        if update.data.is_empty() {
            return Ok(());
        }

        let transaction: Transaction = serde_json::from_slice(&update.data)
            .map_err(|e| format!("Error deserializando transacción: {}", e))?;

        let mut txn_db = transactions_db.write().unwrap();
        txn_db
            .entry(update.account_id)
            .or_default()
            .push(transaction);

        eprintln!(
            "[apply_state_update] [INFO] Transaction applied for account {}",
            update.account_id
        );
        return Ok(());
    }

    // Caso especial: operation_type 253 = Full Sync de transacciones (snapshot)
    if update.operation_type == 253 {
        if update.data.is_empty() {
            return Ok(());
        }

        eprintln!("[apply_state_update] [INFO] Applying full transactions snapshot");
        let txns_map: HashMap<u64, Vec<Transaction>> = serde_json::from_slice(&update.data)
            .map_err(|e| format!("Error deserializando snapshot de transacciones: {}", e))?;

        let mut txn_db = transactions_db.write().unwrap();
        for (account_id, transactions) in txns_map {
            txn_db.insert(account_id, transactions);
        }
        eprintln!(
            "[apply_state_update] [INFO] Full transactions snapshot applied - {} accounts",
            txn_db.len()
        );
        return Ok(());
    }

    // Caso normal: actualización incremental de una cuenta
    if update.data.is_empty() {
        eprintln!(
            "[apply_state_update] WARN: data está vacía para account_id {}",
            update.account_id
        );
        return Ok(()); // No hay datos para aplicar (mensaje antiguo o error)
    }

    // Debug: mostrar JSON recibido
    if let Ok(json_str) = std::str::from_utf8(&update.data) {
        eprintln!(
            "[apply_state_update] Deserializando cuenta {}: {}",
            update.account_id, json_str
        );
    }

    let account: AccountData = serde_json::from_slice(&update.data)
        .map_err(|e| format!("Error deserializando cuenta: {}", e))?;

    // Aplicar el cambio a la DB local (upsert: actualiza o inserta)
    let mut accounts_db = info_db.write().unwrap();
    accounts_db.upsert(update.account_id, account);
    eprintln!(
        "[apply_state_update] [INFO] Account {} applied successfully",
        update.account_id
    );

    Ok(())
}

// TODO: BORRAR!!!! está solo para rellenar
fn populate_test_data(
    _info_db: &Arc<RwLock<AccountDB>>,
    _transactions_db: &Arc<RwLock<HashMap<AccountId, Vec<Transaction>>>>,
) {
    // Datos de prueba deshabilitados por solicitud del usuario
    // El usuario generará sus propios datos de prueba

    /*
    let mut info = info_db.write().unwrap();
    let mut transactions = transactions_db.write().unwrap();

    // Cuenta 1: Juan Pérez
    let account1_id = 1001u64;
    let mut account1 = AccountData::new("juan_perez".to_string(), "Juan Pérez".to_string(), account1_id);
    account1.add_region("CABA".to_string(), 50000.0);
    info.insert(account1_id, account1).expect("Error insertando cuenta de prueba");

    transactions.insert(
        account1_id,
        vec![
            Transaction::new(1700000000i64, 5001u64, 101, 1, 2500.0, 125.0),
            Transaction::new(1700086400i64, 5001u64, 102, 3, 1200.75, 60.0),
            Transaction::new(1700172800i64, 5002u64, 101, 2, 3500.0, 175.0),
        ],
    );

    // Cuenta 2: María González
    let account2_id = 1002u64;
    let mut account2 = AccountData::new("maria_gonzalez".to_string(), "María González".to_string(), account2_id);
    account2.add_region("CABA".to_string(), 75000.0);
    info.insert(account2_id, account2).expect("Error insertando cuenta de prueba");

    transactions.insert(
        account2_id,
        vec![
            Transaction::new(1700050000i64, 5003u64, 103, 1, 3500.0, 175.0),
            Transaction::new(1700136400i64, 5003u64, 104, 4, 890.25, 44.51),
        ],
    );

    // Cuenta 3: Carlos Rodríguez
    let account3_id = 1003u64;
    let mut account3 = AccountData::new("carlos_rodriguez".to_string(), "Carlos Rodríguez".to_string(), account3_id);
    account3.add_region("CABA".to_string(), 100000.0);
    account3.register_card(100.0);
    account3.register_card(200.0);
    info.insert(account3_id, account3).expect("Error insertando cuenta de prueba");

    transactions.insert(
        account3_id,
        vec![
            Transaction::new(1700100000i64, 5004u64, 105, 2, 15000.0, 750.0),
            Transaction::new(1700186400i64, 5005u64, 106, 1, 4567.89, 228.39),
            Transaction::new(1700272800i64, 5004u64, 101, 3, 10000.0, 500.0),
            Transaction::new(1700359200i64, 5006u64, 107, 2, 2345.67, 117.28),
        ],
    );

    // Cuenta 4: Ana Martínez
    let account4_id = 1004u64;
    let mut account4 = AccountData::new("ana_martinez".to_string(), "Ana Martínez".to_string(), account4_id);
    account4.add_region("CABA".to_string(), 30000.0);
    info.insert(account4_id, account4).expect("Error insertando cuenta de prueba");

    transactions.insert(
        account4_id,
        vec![Transaction::new(
            1699900000i64,
            5007u64,
            108,
            4,
            500.0,
            25.0,
        )],
    );
    */
}
