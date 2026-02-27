//! Elección de líder basado en anillo (Chang-Roberts).
//! El nodo con mayor ID se convierte en líder.

use super::heartbeat::{HeartbeatMonitor, HeartbeatSender};
use super::messages::{ElectionMessage, NodeInfoMsg, StateUpdateData};
use chrono::Local;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Mutex, RwLock};
use std::thread;
use std::time::Duration;

/// Log informativo con timestamp
fn log_info(msg: &str) {
    println!("[{}] {}", Local::now().format("%H:%M:%S%.3f"), msg);
}

/// Log de error con timestamp
fn log_error(msg: &str) {
    eprintln!("[{}] {}", Local::now().format("%H:%M:%S%.3f"), msg);
}

/// Información de un nodo en el cluster.
#[derive(Debug, Clone)]
pub struct NodeInfo {
    pub id: u8,
    pub addr: String,        // ring address (ip:ring_port)
    pub client_addr: String, // client address (ip:client_port)
}

impl From<NodeInfoMsg> for NodeInfo {
    fn from(msg: NodeInfoMsg) -> Self {
        NodeInfo {
            id: msg.id,
            addr: msg.addr,
            client_addr: msg.client_addr,
        }
    }
}

impl From<&NodeInfo> for NodeInfoMsg {
    fn from(info: &NodeInfo) -> Self {
        NodeInfoMsg {
            id: info.id,
            addr: info.addr.clone(),
            client_addr: info.client_addr.clone(),
        }
    }
}

/// Nodo en el anillo de elección (Chang-Roberts).
pub struct RingElectionNode {
    pub node_id: u8,
    pub is_leader: Arc<AtomicBool>,
    pub current_leader_id: Arc<AtomicU8>,
    successor_addr: String,
    cluster_nodes: Arc<RwLock<Vec<NodeInfo>>>,
    ring_port: u16,
    is_single_node_ring: bool,
    client_addr: String,
    heartbeat_monitor: Arc<HeartbeatMonitor>,
    heartbeat_sender: Arc<HeartbeatSender>,
    running: Arc<AtomicBool>,
    election_in_progress: Arc<AtomicBool>,
    pub state_update_sender: Option<Sender<StateUpdateData>>,
    heartbeat_error_count: Arc<AtomicU64>,
    last_heartbeat_seq: Arc<Mutex<u64>>,
    primary_successor_failed: Arc<AtomicBool>,
    logged_alternative_use: Arc<AtomicBool>,
}

impl RingElectionNode {
    /// Crea un nuevo nodo para el anillo de elección.
    /// Si `initial_leader_id == 0`, entra en modo descubrimiento.
    pub fn new(
        node_id: u8,
        ring_port: u16,
        successor_addr: String,
        initial_leader_id: u8,
        state_update_sender: Option<Sender<StateUpdateData>>,
        cluster_nodes: Vec<NodeInfo>,
        client_addr: String,
    ) -> Self {
        let is_leader = initial_leader_id != 0 && node_id == initial_leader_id;

        // Detectar si el sucesor apunta a este mismo nodo (ring de un solo nodo)
        let is_single_node_ring = successor_addr.contains(&format!(":{}", ring_port))
            && (successor_addr.starts_with("127.0.0.1:")
                || successor_addr.starts_with("localhost:")
                || successor_addr.starts_with("0.0.0.0:"));

        if is_single_node_ring {
            log_info(&format!(
                "[Ring #{}] [INFO] Ring de un solo nodo detectado - modo standalone",
                node_id
            ));
        }

        Self {
            node_id,
            is_leader: Arc::new(AtomicBool::new(is_leader)),
            current_leader_id: Arc::new(AtomicU8::new(initial_leader_id)),
            successor_addr,
            cluster_nodes: Arc::new(RwLock::new(cluster_nodes)),
            ring_port,
            is_single_node_ring,
            client_addr,
            heartbeat_monitor: Arc::new(HeartbeatMonitor::new(8)),
            heartbeat_sender: Arc::new(HeartbeatSender::new(2)),
            running: Arc::new(AtomicBool::new(true)),
            election_in_progress: Arc::new(AtomicBool::new(false)),
            state_update_sender,
            heartbeat_error_count: Arc::new(AtomicU64::new(0)),
            last_heartbeat_seq: Arc::new(Mutex::new(0)),
            primary_successor_failed: Arc::new(AtomicBool::new(false)),
            logged_alternative_use: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Inicia el nodo del anillo (listeners y monitores).
    pub fn start(self: Arc<Self>) {
        log_info(&format!(
            "[Ring #{}] [INFO] Iniciando nodo del anillo...",
            self.node_id
        ));

        let ring_clone = Arc::clone(&self);
        thread::spawn(move || {
            ring_clone.ring_listener();
        });

        let current_leader = self.current_leader_id.load(Ordering::SeqCst);
        if current_leader == 0 {
            log_info(&format!(
                "[Ring #{}] [INFO] Modo descubrimiento: esperando heartbeats del líder actual...",
                self.node_id
            ));

            let discovery_clone = Arc::clone(&self);
            thread::spawn(move || {
                discovery_clone.leader_discovery();
            });
        } else {
            if !self.is_leader.load(Ordering::SeqCst) {
                let monitor_clone = Arc::clone(&self);
                thread::spawn(move || {
                    monitor_clone.monitor_leader();
                });
            }

            if self.is_leader.load(Ordering::SeqCst) {
                let sender_clone = Arc::clone(&self);
                thread::spawn(move || {
                    sender_clone.send_heartbeats();
                });
            }
        }

        log_info(&format!(
            "[Ring #{}] [INFO] Nodo del anillo iniciado",
            self.node_id
        ));
    }

    fn ring_listener(self: Arc<Self>) {
        let addr = format!("0.0.0.0:{}", self.ring_port);
        let listener = match TcpListener::bind(&addr) {
            Ok(l) => {
                log_info(&format!(
                    "[Ring #{}] [INFO] Escuchando anillo en {}",
                    self.node_id, addr
                ));
                l
            }
            Err(e) => {
                log_error(&format!(
                    "[Ring #{}] [ERROR] Error al bindear puerto del anillo: {}",
                    self.node_id, e
                ));
                return;
            }
        };

        for stream in listener.incoming() {
            if !self.running.load(Ordering::SeqCst) {
                break;
            }

            match stream {
                Ok(stream) => {
                    let node = Arc::clone(&self);
                    thread::spawn(move || {
                        node.handle_ring_connection(stream);
                    });
                }
                Err(e) => {
                    log_error(&format!(
                        "[Ring #{}] Error aceptando conexión: {}",
                        self.node_id, e
                    ));
                }
            }
        }
    }

    fn handle_ring_connection(&self, stream: TcpStream) {
        let mut reader = BufReader::new(stream);
        let mut line = String::new();

        // Leer UNA línea (un mensaje por conexión)
        match reader.read_line(&mut line) {
            Ok(0) => {
                // Conexión cerrada sin datos
            }
            Ok(_) => {
                // Procesar el mensaje recibido
                if let Ok(msg) = ElectionMessage::from_json(line.trim()) {
                    self.handle_ring_message(msg);
                }
            }
            Err(_) => {
                // Error leyendo, ignorar silenciosamente
            }
        }
    }

    fn handle_ring_message(&self, msg: ElectionMessage) {
        match msg {
            ElectionMessage::Election {
                candidate_id,
                visited_nodes,
            } => {
                self.handle_election(candidate_id, visited_nodes);
            }
            ElectionMessage::Elected {
                leader_id,
                cluster_nodes,
            } => {
                self.handle_elected(leader_id, cluster_nodes);
            }
            ElectionMessage::Heartbeat {
                leader_id,
                sequence,
            } => {
                self.handle_heartbeat(leader_id, sequence);
            }
            ElectionMessage::ClusterSync {
                cluster_nodes,
                leader_id,
                origin_id,
                client_addr: _,
            } => {
                self.handle_cluster_sync(cluster_nodes, leader_id, origin_id);
            }
            ElectionMessage::StateUpdate {
                operation_type,
                account_id,
                data,
                timestamp,
            } => {
                self.handle_state_update(operation_type, account_id, data, timestamp);
            }
            ElectionMessage::StateSyncRequest { requesting_node_id } => {
                // Ignorar - esto se maneja por el thread de snapshots periódicos
                log_info(&format!(
                    "[Ring #{}] [INFO] StateSyncRequest de nodo {} recibido (se sincronizará con próximo snapshot)",
                    self.node_id, requesting_node_id
                ));
            }
            ElectionMessage::StateSyncResponse { accounts } => {
                // Recibir snapshot completo y aplicarlo al canal (Thread 3 lo procesará)
                if !self.is_leader.load(Ordering::SeqCst)
                    && let Some(ref sender) = self.state_update_sender
                {
                    // Enviar un update especial con operation_type = 255 para indicar "full sync"
                    let sync_update = StateUpdateData {
                        operation_type: 255, // Código especial para full sync
                        account_id: 0,
                        data: accounts,
                        timestamp: 0,
                    };
                    let _ = sender.send(sync_update);
                    log_info(&format!(
                        "[Ring #{}] [INFO] Snapshot completo recibido del líder",
                        self.node_id
                    ));
                }
            }
        }
    }

    fn handle_election(&self, candidate_id: u8, visited_nodes: Vec<NodeInfoMsg>) {
        // Marcar que hay una elección en progreso
        self.election_in_progress.store(true, Ordering::SeqCst);

        // CASO ESPECIAL: Si recibo un mensaje de elección donde YO soy el candidato
        // y ya pasé por el anillo (estoy en visited_nodes), significa que gané
        if candidate_id == self.node_id && visited_nodes.iter().any(|n| n.id == self.node_id) {
            log_info(&format!(
                "[Ring #{}] [INFO] Mi mensaje Election({}) volvió - \x1b[32mSoy el líder con cluster completo\x1b[0m",
                self.node_id, self.node_id
            ));

            // Actualizar lista del cluster
            let nodes: Vec<NodeInfo> = visited_nodes.iter().map(|n| n.clone().into()).collect();
            *self.cluster_nodes.write().unwrap() = nodes.clone();

            self.become_leader_with_cluster(visited_nodes);
            return;
        }

        // Verificar si ya estoy en la lista de nodos visitados
        if visited_nodes.iter().any(|n| n.id == self.node_id) {
            // El mensaje completó un círculo, pero el candidato NO soy yo
            // Esto significa que otro nodo con mayor ID debe ganar
            log_info(&format!(
                "[Ring #{}] [INFO] Descubrimiento completado - {} nodos (candidato: Nodo #{})",
                self.node_id,
                visited_nodes.len(),
                candidate_id
            ));

            // Actualizar mi lista de cluster_nodes con la lista completa
            let nodes: Vec<NodeInfo> = visited_nodes.iter().map(|n| n.clone().into()).collect();
            *self.cluster_nodes.write().unwrap() = nodes.clone();

            let msg = ElectionMessage::Election {
                candidate_id,
                visited_nodes,
            };
            let _ = self.send_to_successor(&msg);

            self.election_in_progress.store(false, Ordering::SeqCst);
            return;
        }

        let mut updated_visited = visited_nodes.clone();
        updated_visited.push(NodeInfoMsg {
            id: self.node_id,
            addr: format!("127.0.0.1:{}", self.ring_port),
            client_addr: self.client_addr.clone(),
        });

        let new_candidate_id = if candidate_id > self.node_id {
            log_info(&format!(
                "[Ring #{}] [INFO] Recibido Election({}) - Propagando (es mayor)",
                self.node_id, candidate_id
            ));
            candidate_id
        } else {
            log_info(&format!(
                "[Ring #{}] [INFO] Recibido Election({}) - Reemplazando con mi ID ({})",
                self.node_id, candidate_id, self.node_id
            ));
            self.node_id
        };

        let msg = ElectionMessage::Election {
            candidate_id: new_candidate_id,
            visited_nodes: updated_visited,
        };
        let _ = self.send_to_successor(&msg);
    }

    fn handle_elected(&self, leader_id: u8, cluster_nodes: Vec<NodeInfoMsg>) {
        if leader_id == self.node_id {
            self.election_in_progress.store(false, Ordering::SeqCst);
            return;
        }

        log_info(&format!(
            "[Ring #{}] [INFO] Recibido anuncio: Líder #{} fue elegido",
            self.node_id, leader_id
        ));

        let nodes: Vec<NodeInfo> = cluster_nodes.iter().map(|n| n.clone().into()).collect();
        *self.cluster_nodes.write().unwrap() = nodes.clone();

        log_info(&format!(
            "[Ring #{}] [INFO] Lista del cluster actualizada: {} nodos",
            self.node_id,
            nodes.len()
        ));
        for node in &nodes {
            let marker = if node.id == self.node_id {
                " \x1b[32m(yo)\x1b[0m"
            } else {
                ""
            };
            log_info(&format!(
                "[Ring #{}]    • Nodo #{}: {}{}",
                self.node_id, node.id, node.addr, marker
            ));
        }

        self.current_leader_id.store(leader_id, Ordering::SeqCst);
        self.is_leader.store(false, Ordering::SeqCst);
        self.election_in_progress.store(false, Ordering::SeqCst);
        self.heartbeat_monitor.update();

        if !self.heartbeat_monitor.is_running() {
            let monitor_clone = Arc::new(self.clone_minimal());
            thread::spawn(move || {
                let node = RingElectionNode {
                    node_id: monitor_clone.node_id,
                    is_leader: Arc::clone(&monitor_clone.is_leader),
                    current_leader_id: Arc::clone(&monitor_clone.current_leader_id),
                    successor_addr: monitor_clone.successor_addr.clone(),
                    cluster_nodes: Arc::clone(&monitor_clone.cluster_nodes),
                    ring_port: monitor_clone.ring_port,
                    is_single_node_ring: monitor_clone.is_single_node_ring,
                    client_addr: monitor_clone.client_addr.clone(),
                    heartbeat_monitor: Arc::clone(&monitor_clone.heartbeat_monitor),
                    heartbeat_sender: Arc::clone(&monitor_clone.heartbeat_sender),
                    running: Arc::clone(&monitor_clone.running),
                    election_in_progress: Arc::clone(&monitor_clone.election_in_progress),
                    state_update_sender: monitor_clone.state_update_sender.clone(),
                    heartbeat_error_count: Arc::clone(&monitor_clone.heartbeat_error_count),
                    last_heartbeat_seq: Arc::clone(&monitor_clone.last_heartbeat_seq),
                    primary_successor_failed: Arc::clone(&monitor_clone.primary_successor_failed),
                    logged_alternative_use: Arc::clone(&monitor_clone.logged_alternative_use),
                };
                node.monitor_leader();
            });
        }

        let msg = ElectionMessage::Elected {
            leader_id,
            cluster_nodes,
        };
        let _ = self.send_to_successor(&msg);
    }

    fn handle_heartbeat(&self, leader_id: u8, sequence: u64) {
        if self.is_leader.load(Ordering::SeqCst) && leader_id != self.node_id {
            if leader_id > self.node_id {
                self.is_leader.store(false, Ordering::SeqCst);
                self.current_leader_id.store(leader_id, Ordering::SeqCst);
                self.heartbeat_monitor.update();
                self.election_in_progress.store(false, Ordering::SeqCst);
            } else {
                // Yo tengo mayor ID, ignoro el heartbeat
                return;
            }
        }

        if !self.is_leader.load(Ordering::SeqCst) {
            self.heartbeat_monitor.update();
            let prev_leader = self.current_leader_id.load(Ordering::SeqCst);
            if prev_leader != leader_id {
                // Primer heartbeat de este líder - importante loguearlo
                if prev_leader == 0 {
                    log_info(&format!(
                        "[Ring #{}] [INFO] Primer heartbeat recibido del líder #{} (descubrimiento)",
                        self.node_id, leader_id
                    ));
                } else {
                    log_info(&format!(
                        "[Ring #{}] [INFO] Nuevo líder detectado: #{} (anterior: #{})",
                        self.node_id, leader_id, prev_leader
                    ));
                }
                self.current_leader_id.store(leader_id, Ordering::SeqCst);
                self.election_in_progress.store(false, Ordering::SeqCst); // Cancelar cualquier elección

                // Solicitar sincronización de topología al nuevo líder
                // (El líder responderá con ClusterSync cuando reciba el próximo heartbeat)
                let nodes = self.cluster_nodes.read().unwrap();
                if nodes.len() <= 1 {
                    // Solo me conozco a mí mismo, necesito sincronización
                    log_info(&format!(
                        "[Ring #{}] [INFO] Solicitando topología completa del cluster...",
                        self.node_id
                    ));
                }
                drop(nodes);
            }
        }

        // Propagar heartbeat SOLO si es nuevo (no lo hemos visto antes)
        // Esto evita que el heartbeat circule infinitamente
        let mut last_seq = self.last_heartbeat_seq.lock().unwrap();
        if sequence > *last_seq {
            *last_seq = sequence;
            drop(last_seq); // Liberar lock antes de enviar

            // Propagar al siguiente nodo
            match self.send_to_successor(&ElectionMessage::Heartbeat {
                leader_id,
                sequence,
            }) {
                Ok(_) => {
                    // Heartbeat propagado exitosamente (no loguear para evitar spam)
                }
                Err(e) => {
                    // Error al propagar - esto es importante
                    log_error(&format!(
                        "[Ring #{}] [WARN] Error propagando heartbeat seq={}: {}",
                        self.node_id, sequence, e
                    ));
                }
            }
        } else {
            // Heartbeat duplicado o viejo, no propagar
            // Esto es normal cuando el heartbeat completa el círculo
        }
    }

    fn become_leader(&self) {
        // Verificar si ya soy líder para no iniciar múltiples threads
        if self.is_leader.load(Ordering::SeqCst) {
            return; // Ya soy líder, no hacer nada
        }

        self.is_leader.store(true, Ordering::SeqCst);
        self.current_leader_id.store(self.node_id, Ordering::SeqCst);
        self.election_in_progress.store(false, Ordering::SeqCst);

        log_info(&format!(
            "[Ring #{}] [INFO] Ahora soy el líder",
            self.node_id
        ));

        // Obtener lista actual del cluster (puede estar parcial si no se completó descubrimiento)
        let nodes = self.cluster_nodes.read().unwrap();
        let cluster_msg: Vec<NodeInfoMsg> = nodes.iter().map(|n| n.into()).collect();
        drop(nodes);

        // Anuncio al anillo
        let msg = ElectionMessage::Elected {
            leader_id: self.node_id,
            cluster_nodes: cluster_msg,
        };
        let _ = self.send_to_successor(&msg);

        // Inicio thread de heartbeats
        let sender_clone = Arc::new(self.clone_minimal());
        thread::spawn(move || {
            let node = RingElectionNode {
                node_id: sender_clone.node_id,
                is_leader: Arc::clone(&sender_clone.is_leader),
                current_leader_id: Arc::clone(&sender_clone.current_leader_id),
                successor_addr: sender_clone.successor_addr.clone(),
                cluster_nodes: Arc::clone(&sender_clone.cluster_nodes),
                ring_port: sender_clone.ring_port,
                is_single_node_ring: sender_clone.is_single_node_ring,
                client_addr: sender_clone.client_addr.clone(),
                heartbeat_monitor: Arc::clone(&sender_clone.heartbeat_monitor),
                heartbeat_sender: Arc::clone(&sender_clone.heartbeat_sender),
                running: Arc::clone(&sender_clone.running),
                election_in_progress: Arc::clone(&sender_clone.election_in_progress),
                state_update_sender: sender_clone.state_update_sender.clone(),
                heartbeat_error_count: Arc::clone(&sender_clone.heartbeat_error_count),
                last_heartbeat_seq: Arc::clone(&sender_clone.last_heartbeat_seq),
                primary_successor_failed: Arc::clone(&sender_clone.primary_successor_failed),
                logged_alternative_use: Arc::clone(&sender_clone.logged_alternative_use),
            };
            node.send_heartbeats();
        });
    }

    fn become_leader_with_cluster(&self, cluster_nodes: Vec<NodeInfoMsg>) {
        // Verificar si ya soy líder para no iniciar múltiples threads
        if self.is_leader.load(Ordering::SeqCst) {
            return; // Ya soy líder, no hacer nada
        }

        self.is_leader.store(true, Ordering::SeqCst);
        self.current_leader_id.store(self.node_id, Ordering::SeqCst);
        self.election_in_progress.store(false, Ordering::SeqCst);

        log_info(&format!(
            "[Ring #{}] [INFO] Ahora soy el líder con cluster completo ({} nodos)",
            self.node_id,
            cluster_nodes.len()
        ));

        // Anuncio al anillo con la lista completa
        let msg = ElectionMessage::Elected {
            leader_id: self.node_id,
            cluster_nodes,
        };
        let _ = self.send_to_successor(&msg);

        // Inicio thread de heartbeats
        let sender_clone = Arc::new(self.clone_minimal());
        thread::spawn(move || {
            let node = RingElectionNode {
                node_id: sender_clone.node_id,
                is_leader: Arc::clone(&sender_clone.is_leader),
                current_leader_id: Arc::clone(&sender_clone.current_leader_id),
                successor_addr: sender_clone.successor_addr.clone(),
                cluster_nodes: Arc::clone(&sender_clone.cluster_nodes),
                ring_port: sender_clone.ring_port,
                is_single_node_ring: sender_clone.is_single_node_ring,
                client_addr: sender_clone.client_addr.clone(),
                heartbeat_monitor: Arc::clone(&sender_clone.heartbeat_monitor),
                heartbeat_sender: Arc::clone(&sender_clone.heartbeat_sender),
                running: Arc::clone(&sender_clone.running),
                election_in_progress: Arc::clone(&sender_clone.election_in_progress),
                state_update_sender: sender_clone.state_update_sender.clone(),
                heartbeat_error_count: Arc::clone(&sender_clone.heartbeat_error_count),
                last_heartbeat_seq: Arc::clone(&sender_clone.last_heartbeat_seq),
                primary_successor_failed: Arc::clone(&sender_clone.primary_successor_failed),
                logged_alternative_use: Arc::clone(&sender_clone.logged_alternative_use),
            };
            node.send_heartbeats();
        });
    }

    /// Inicia una elección enviando mi ID al anillo.
    pub fn start_election(&self) {
        if self.election_in_progress.load(Ordering::SeqCst) {
            return; // Silenciosamente ignorar si ya hay elección
        }

        self.election_in_progress.store(true, Ordering::SeqCst);
        log_info(&format!(
            "[Ring #{}] [INFO] Iniciando elección. Mi ID: {}",
            self.node_id, self.node_id
        ));

        // Crear lista inicial con mi información
        let visited_nodes = vec![NodeInfoMsg {
            id: self.node_id,
            addr: format!("127.0.0.1:{}", self.ring_port),
            client_addr: self.client_addr.clone(),
        }];

        let msg = ElectionMessage::Election {
            candidate_id: self.node_id,
            visited_nodes,
        };

        if self.send_to_successor(&msg).is_err() {
            // Si falla el envío, resetear el estado de elección
            self.election_in_progress.store(false, Ordering::SeqCst);
        }

        // Auto-finalizar elección después de 15 segundos si no hay respuesta
        let node_clone = Arc::new(self.clone_minimal());
        thread::spawn(move || {
            thread::sleep(Duration::from_secs(15));
            node_clone
                .election_in_progress
                .store(false, Ordering::SeqCst);
        });
    }

    pub fn get_leader_addr(&self) -> Option<String> {
        let leader_id = self.current_leader_id.load(Ordering::SeqCst);
        let nodes = self.cluster_nodes.read().unwrap();

        for node in nodes.iter() {
            if leader_id == node.id {
                return Some(node.client_addr.to_string());
            }
        }
        None
    }

    /// Monitorea al líder actual mediante heartbeats.
    fn monitor_leader(&self) {
        log_info(&format!(
            "[Ring #{}] [INFO] Iniciando monitor de heartbeats",
            self.node_id
        ));

        loop {
            thread::sleep(Duration::from_secs(5)); // Chequeo menos frecuente

            if !self.running.load(Ordering::SeqCst) {
                println!(
                    "[Ring #{}] Deteniendo monitor de heartbeats por running ord",
                    self.node_id
                );
                break;
            }

            // Si soy líder, no monitoreo
            if self.is_leader.load(Ordering::SeqCst) {
                println!(
                    "[Ring #{}] Soy líder, deteniendo monitor de heartbeats",
                    self.node_id
                );
                break;
            }

            // Verifico timeout solo si NO hay elección en progreso
            if self.heartbeat_monitor.is_timeout()
                && !self.election_in_progress.load(Ordering::SeqCst)
            {
                log_info(&format!(
                    "[Ring #{}] [WARN] \x1b[31mTIMEOUT: Líder no responde.\x1b[0m Iniciando elección...",
                    self.node_id
                ));

                // Backoff aleatorio basado en node_id para evitar elecciones simultáneas
                // Nodos con menor ID esperan más, dando prioridad a nodos con mayor ID
                let backoff_ms = (255 - self.node_id) as u64 * 50; // 0-12750ms
                if backoff_ms > 0 {
                    thread::sleep(Duration::from_millis(backoff_ms));
                }

                // Verificar nuevamente si ya hay una elección en progreso
                // (otro nodo podría haberla iniciado durante el backoff)
                if !self.election_in_progress.load(Ordering::SeqCst) {
                    self.start_election();
                }

                // Esperar a que termine la elección antes de volver a monitorear
                thread::sleep(Duration::from_secs(20));

                // Reset del monitor para dar tiempo
                self.heartbeat_monitor.update();
            }
        }

        log_info(&format!(
            "[Ring #{}] Monitor de heartbeats finalizado",
            self.node_id
        ));
    }

    /// Envía heartbeats periódicamente (solo el líder).
    fn send_heartbeats(&self) {
        log_info(&format!(
            "[Ring #{}] [INFO] Iniciando envío de heartbeats como líder",
            self.node_id
        ));

        loop {
            thread::sleep(self.heartbeat_sender.interval());

            if !self.running.load(Ordering::SeqCst) {
                break;
            }

            // Si ya no soy líder, detengo el envío
            if !self.is_leader.load(Ordering::SeqCst) {
                println!("[Ring #3] \x1b[32mSoy líder\x1b[0m, deteniendo monitor de heartbeats");
                break;
            }

            let sequence = self.heartbeat_sender.next_sequence();

            // Actualizar mi propio last_heartbeat_seq para evitar repropagar
            // mi propio heartbeat cuando vuelva al completar el círculo
            {
                let mut last_seq = self.last_heartbeat_seq.lock().unwrap();
                *last_seq = sequence;
            }

            let msg = ElectionMessage::Heartbeat {
                leader_id: self.node_id,
                sequence,
            };

            match self.send_to_successor(&msg) {
                Ok(_) => {
                    // Éxito: resetear contador de errores
                    self.heartbeat_error_count.store(0, Ordering::SeqCst);
                }
                Err(e) => {
                    // Error: incrementar contador y solo logear ocasionalmente
                    let count = self.heartbeat_error_count.fetch_add(1, Ordering::SeqCst) + 1;

                    // Solo logear el primer error y luego cada 10 errores
                    if count == 1 || count.is_multiple_of(10) {
                        log_error(&format!(
                            "[Ring #{}] [ERROR] Error enviando heartbeat (x{}): {}",
                            self.node_id, count, e
                        ));
                    }
                }
            }

            // Cada 3 heartbeats (15 segundos), enviar ClusterSync para asegurar
            // que todos los nodos (especialmente los que se reintegran) tengan la topología completa
            if sequence.is_multiple_of(3) {
                self.broadcast_cluster_sync();
            }
        }

        log_info(&format!(
            "[Ring #{}] Envío de heartbeats finalizado",
            self.node_id
        ));
    }

    /// Retorna la lista de nodos candidatos en orden circular para intentar conexión.
    /// No hace health check, solo retorna el orden correcto para que el caller pruebe.
    /// Si skip_primary_successor=true, excluye el sucesor configurado originalmente.
    fn get_candidate_nodes(&self, skip_primary_successor: bool) -> Vec<(String, u8)> {
        let nodes = self.cluster_nodes.read().unwrap();

        // Construir lista de nodos en orden circular después de mi ID
        // Por ejemplo, si soy Nodo 2 en [1,2,3,4], el orden circular es: [3,4,1]
        let mut circular_order: Vec<_> = nodes.iter().filter(|n| n.id != self.node_id).collect();

        if circular_order.is_empty() {
            return Vec::new();
        }

        // Ordenar y reorganizar para orden circular
        circular_order.sort_by_key(|n| n.id);

        // Separar en: nodos después de mí + nodos antes de mí
        let split_pos = circular_order
            .iter()
            .position(|n| n.id > self.node_id)
            .unwrap_or(circular_order.len());

        let mut ordered_nodes = Vec::new();
        // Primero los nodos con ID mayor (mis sucesores naturales)
        ordered_nodes.extend_from_slice(&circular_order[split_pos..]);
        // Luego los nodos con ID menor (wrap around)
        ordered_nodes.extend_from_slice(&circular_order[..split_pos]);

        // Convertir a (addr, id)
        let mut candidates: Vec<_> = ordered_nodes
            .iter()
            .map(|n| (n.addr.clone(), n.id))
            .collect();

        // Si se solicita skip_primary_successor, filtrar el sucesor configurado
        if skip_primary_successor {
            // El sucesor primario es el que está configurado en successor_addr
            // Necesitamos excluirlo de la lista
            candidates.retain(|(addr, _)| addr != &self.successor_addr);
        }

        candidates
    }

    /// Devuelve una lista de todos los addrs presentes en el cluster.
    pub fn get_cluster_client_addrs(&self) -> Vec<String> {
        let mut addrs = Vec::new();
        let nodes = self.cluster_nodes.read().unwrap();

        for node in nodes.iter() {
            addrs.push(node.client_addr.to_string());
        }
        addrs
    }

    /// Intenta enviar un mensaje a una dirección específica.
    fn try_send(&self, addr: &str, msg: &ElectionMessage) -> Result<(), String> {
        let stream_result = TcpStream::connect_timeout(
            &addr.parse().map_err(|_| "Dirección inválida".to_string())?,
            Duration::from_secs(3),
        );

        let mut stream = stream_result.map_err(|e| format!("Conexión falló: {}", e))?;

        let json = msg.to_json();
        stream
            .write_all(json.as_bytes())
            .map_err(|e| format!("Error escribiendo: {}", e))?;
        stream
            .write_all(b"\n")
            .map_err(|e| format!("Error escribiendo newline: {}", e))?;
        stream.flush().map_err(|e| format!("Error flush: {}", e))?;

        Ok(())
    }

    /// Envía un mensaje al sucesor en el anillo.
    /// Si el sucesor no está disponible (nodo caído), intenta con el siguiente nodo vivo.
    fn send_to_successor(&self, msg: &ElectionMessage) -> Result<(), String> {
        // Si es un ring de un solo nodo, no intentar enviar
        if self.is_single_node_ring {
            // Para mensajes de elección, auto-elegirse inmediatamente
            if let ElectionMessage::Election { candidate_id, .. } = msg
                && *candidate_id == self.node_id
            {
                self.become_leader();
            }
            // Los demás mensajes (Heartbeat, Elected, etc.) se ignoran silenciosamente
            // ya que no hay otros nodos a los que enviar
            return Ok(());
        }

        // Determinar tipo de mensaje para logging
        let msg_type = match msg {
            ElectionMessage::Election { candidate_id, .. } => format!("Election({})", candidate_id),
            ElectionMessage::Elected { leader_id, .. } => format!("Elected({})", leader_id),
            ElectionMessage::Heartbeat {
                leader_id,
                sequence,
            } => format!("Heartbeat({}, seq:{})", leader_id, sequence),
            ElectionMessage::ClusterSync { origin_id, .. } => {
                format!("ClusterSync(origin:{})", origin_id)
            }
            ElectionMessage::StateUpdate { .. } => {
                // StateUpdate NO debe pasar por send_to_successor del ring
                // Si llega aquí, es un error de programación
                log_info(&format!(
                    "[Ring #{}] [ERROR] StateUpdate no debe usar send_to_successor del ring!",
                    self.node_id
                ));
                return Err("StateUpdate debe manejarse por el Thread 3 de replicación".to_string());
            }
            ElectionMessage::StateSyncRequest { requesting_node_id } => {
                format!("StateSyncRequest(from:{})", requesting_node_id)
            }
            ElectionMessage::StateSyncResponse { .. } => "StateSyncResponse".to_string(),
        };

        // Primero, intentar con el sucesor primario
        match self.try_send(&self.successor_addr, msg) {
            Ok(_) => {
                // Éxito con el sucesor primario
                let prev_error_count = self.heartbeat_error_count.swap(0, Ordering::SeqCst);

                // Solo loguear recuperación si hubo fallos previos (evita spam con 1-2 fallos transitorios)
                if prev_error_count >= 3 {
                    log_info(&format!(
                        "[Ring #{}] [INFO] Primary successor ({}) recovered (after {} failures)",
                        self.node_id, self.successor_addr, prev_error_count
                    ));

                    // Resetear flags
                    self.primary_successor_failed.store(false, Ordering::SeqCst);
                    self.logged_alternative_use.store(false, Ordering::SeqCst);

                    // Enviar sincronización de topología a través del anillo
                    self.broadcast_cluster_sync();
                }
                Ok(())
            }
            Err(_) => {
                // Incrementar contador de errores
                let error_count = self.heartbeat_error_count.fetch_add(1, Ordering::SeqCst) + 1;

                // Loguear SIEMPRE el primer fallo (detección inmediata de caída de nodo)
                if error_count == 1 {
                    log_info(&format!(
                        "[Ring #{}] [WARN] Primary successor ({}) not responding, searching alternatives",
                        self.node_id, self.successor_addr
                    ));
                    self.primary_successor_failed.store(true, Ordering::SeqCst);
                }

                // CASO ESPECIAL: Si estoy enviando MI PROPIO ID en una elección
                // y el sucesor primario falla, intentar con otros nodos antes de auto-elegirme
                if let ElectionMessage::Election { candidate_id, .. } = msg
                    && *candidate_id == self.node_id
                {
                    // Intentar con todos los candidatos en orden
                    let candidates = self.get_candidate_nodes(true);

                    for (i, (next_addr, next_id)) in candidates.iter().enumerate() {
                        // Pequeño delay entre reintentos para no saturar las conexiones
                        if i > 0 {
                            thread::sleep(Duration::from_millis(50));
                        }

                        // Loguear SIEMPRE el primer intento con alternativo
                        if i == 0 {
                            log_info(&format!(
                                "[Ring #{}] [INFO] Attempting with Node #{} ({})",
                                self.node_id, next_id, next_addr
                            ));
                        }

                        match self.try_send(next_addr, msg) {
                            Ok(_) => {
                                if i > 0 {
                                    log_info(&format!(
                                        "[Ring #{}] [INFO] Node #{} responded correctly",
                                        self.node_id, next_id
                                    ));
                                } else {
                                    log_info(&format!(
                                        "[Ring #{}] [INFO] Node #{} (alternative) operational",
                                        self.node_id, next_id
                                    ));
                                }
                                self.heartbeat_error_count.store(0, Ordering::SeqCst);
                                return Ok(());
                            }
                            Err(_) => {
                                if i < candidates.len() - 1 {
                                    log_info(&format!(
                                        "[Ring #{}] [WARN] Node #{} not responding, trying next",
                                        self.node_id, next_id
                                    ));
                                } else {
                                    log_info(&format!(
                                        "[Ring #{}] [ERROR] Node #{} not responding",
                                        self.node_id, next_id
                                    ));
                                }
                                continue;
                            }
                        }
                    }

                    // No hay otros nodos vivos, soy el líder
                    log_info(&format!(
                        "[Ring #{}] [INFO] No alternative node responding ({} candidates tried). Self-electing as leader",
                        self.node_id,
                        candidates.len()
                    ));
                    self.become_leader();
                    return Ok(());
                }

                // Para otros mensajes o IDs, intentar con todos los candidatos en orden
                let candidates = self.get_candidate_nodes(true);

                for (i, (next_addr, next_id)) in candidates.iter().enumerate() {
                    // Pequeño delay entre reintentos para no saturar las conexiones
                    if i > 0 {
                        thread::sleep(Duration::from_millis(50));
                    }

                    match self.try_send(next_addr, msg) {
                        Ok(_) => {
                            // Loguear SIEMPRE cuando se usa un alternativo (transparencia del routing)
                            if i == 0 && !self.logged_alternative_use.swap(true, Ordering::SeqCst) {
                                log_info(&format!(
                                    "[Ring #{}] [INFO] Using Node #{} as alternative",
                                    self.node_id, next_id
                                ));
                            } else if i > 0 {
                                log_info(&format!(
                                    "[Ring #{}] [INFO] Node #{} responded (secondary alternative)",
                                    self.node_id, next_id
                                ));
                            }
                            self.heartbeat_error_count.store(0, Ordering::SeqCst);
                            return Ok(());
                        }
                        Err(_) => {
                            // Este nodo no respondió, loguear para debugging
                            if i < candidates.len() - 1 {
                                log_info(&format!(
                                    "[Ring #{}] [WARN] Node #{} not responding, trying next",
                                    self.node_id, next_id
                                ));
                            }
                            continue;
                        }
                    }
                }

                // Ningún nodo respondió después de intentar con todos los candidatos
                let cluster_size = self.cluster_nodes.read().unwrap().len();
                log_error(&format!(
                    "[Ring #{}] [ERROR] CRITICAL: No cluster node responding (cluster size: {}, candidates tried: {})",
                    self.node_id,
                    cluster_size,
                    candidates.len()
                ));
                Err(format!(
                    "No hay nodos vivos en el cluster para enviar {}",
                    msg_type
                ))
            }
        }
    }

    /// Thread de descubrimiento del líder actual.
    /// Espera heartbeats por un tiempo limitado antes de iniciar elección.
    /// Usa timeout escalonado según node_id para evitar elecciones simultáneas.
    fn leader_discovery(&self) {
        // Timeout optimizado para localhost (3s base + 1s por nodo)
        let base_timeout = Duration::from_secs(3);
        let node_offset = Duration::from_secs(self.node_id as u64);
        let discovery_timeout = base_timeout + node_offset;

        let check_interval = Duration::from_millis(500);
        let start_time = std::time::Instant::now();

        while start_time.elapsed() < discovery_timeout {
            thread::sleep(check_interval);

            // Si ya descubrimos al líder, salir
            let current_leader = self.current_leader_id.load(Ordering::SeqCst);
            if current_leader != 0 {
                log_info(&format!(
                    "[Ring #{}] [INFO] Descubrimiento completado. Líder conocido: #{}",
                    self.node_id, current_leader
                ));

                // Iniciar monitor de heartbeats
                let monitor_clone = Arc::new(self.clone_minimal());
                thread::spawn(move || {
                    let node = RingElectionNode {
                        node_id: monitor_clone.node_id,
                        is_leader: Arc::clone(&monitor_clone.is_leader),
                        current_leader_id: Arc::clone(&monitor_clone.current_leader_id),
                        successor_addr: monitor_clone.successor_addr.clone(),
                        cluster_nodes: Arc::clone(&monitor_clone.cluster_nodes),
                        ring_port: monitor_clone.ring_port,
                        is_single_node_ring: monitor_clone.is_single_node_ring,
                        client_addr: monitor_clone.client_addr.clone(),
                        heartbeat_monitor: Arc::clone(&monitor_clone.heartbeat_monitor),
                        heartbeat_sender: Arc::clone(&monitor_clone.heartbeat_sender),
                        running: Arc::clone(&monitor_clone.running),
                        election_in_progress: Arc::clone(&monitor_clone.election_in_progress),
                        state_update_sender: monitor_clone.state_update_sender.clone(),
                        heartbeat_error_count: Arc::clone(&monitor_clone.heartbeat_error_count),
                        last_heartbeat_seq: Arc::clone(&monitor_clone.last_heartbeat_seq),
                        primary_successor_failed: Arc::clone(
                            &monitor_clone.primary_successor_failed,
                        ),
                        logged_alternative_use: Arc::clone(&monitor_clone.logged_alternative_use),
                    };
                    node.monitor_leader();
                });
                return;
            }
        }

        // Timeout: no se descubrió líder, iniciar elección
        log_info(&format!(
            "[Ring #{}] [WARN] Timeout de descubrimiento ({:?}). No hay líder activo, iniciando elección...",
            self.node_id, discovery_timeout
        ));
        self.start_election();

        // Después de la elección, iniciar monitor si no soy líder
        thread::sleep(Duration::from_secs(3));
        if !self.is_leader.load(Ordering::SeqCst) {
            let monitor_clone = Arc::new(self.clone_minimal());
            thread::spawn(move || {
                let node = RingElectionNode {
                    node_id: monitor_clone.node_id,
                    is_leader: Arc::clone(&monitor_clone.is_leader),
                    current_leader_id: Arc::clone(&monitor_clone.current_leader_id),
                    successor_addr: monitor_clone.successor_addr.clone(),
                    cluster_nodes: Arc::clone(&monitor_clone.cluster_nodes),
                    ring_port: monitor_clone.ring_port,
                    is_single_node_ring: monitor_clone.is_single_node_ring,
                    client_addr: monitor_clone.client_addr.clone(),
                    heartbeat_monitor: Arc::clone(&monitor_clone.heartbeat_monitor),
                    heartbeat_sender: Arc::clone(&monitor_clone.heartbeat_sender),
                    running: Arc::clone(&monitor_clone.running),
                    election_in_progress: Arc::clone(&monitor_clone.election_in_progress),
                    state_update_sender: monitor_clone.state_update_sender.clone(),
                    heartbeat_error_count: Arc::clone(&monitor_clone.heartbeat_error_count),
                    last_heartbeat_seq: Arc::clone(&monitor_clone.last_heartbeat_seq),
                    primary_successor_failed: Arc::clone(&monitor_clone.primary_successor_failed),
                    logged_alternative_use: Arc::clone(&monitor_clone.logged_alternative_use),
                };
                node.monitor_leader();
            });
        }
    }

    fn handle_cluster_sync(&self, cluster_nodes: Vec<NodeInfoMsg>, leader_id: u8, origin_id: u8) {
        // Si el mensaje volvió a su origen, detener propagación (sin log, es rutinario)
        if origin_id == self.node_id {
            return;
        }

        // Guardar estado anterior para detectar cambios
        let old_nodes = self.cluster_nodes.read().unwrap();
        let mut old_node_ids: Vec<u8> = old_nodes.iter().map(|n| n.id).collect();
        old_node_ids.sort(); // Asegurar que esté ordenado para comparación
        let old_leader = self.current_leader_id.load(Ordering::SeqCst);
        drop(old_nodes);

        // Actualizar lista completa del cluster
        let mut nodes: Vec<NodeInfo> = cluster_nodes.iter().map(|n| n.clone().into()).collect();

        // IMPORTANTE: Asegurar que YO siempre esté en mi propia lista
        if !nodes.iter().any(|n| n.id == self.node_id) {
            nodes.push(NodeInfo {
                id: self.node_id,
                addr: format!("127.0.0.1:{}", self.ring_port),
                client_addr: self.client_addr.clone(),
            });
        }

        // Ordenar por ID para consistencia
        nodes.sort_by_key(|n| n.id);

        // Detectar si hubo cambios REALES en la topología (después de agregar a mí mismo)
        let new_node_ids: Vec<u8> = nodes.iter().map(|n| n.id).collect();
        let topology_changed = old_node_ids != new_node_ids;
        let leader_changed = old_leader != leader_id;

        *self.cluster_nodes.write().unwrap() = nodes.clone();

        // Solo imprimir si hay CAMBIOS reales (topología o líder)
        if topology_changed || leader_changed {
            log_info(&format!(
                "[Ring #{}] [INFO] Sincronización de topología recibida",
                self.node_id
            ));

            if topology_changed {
                log_info(&format!(
                    "[Ring #{}] [INFO] Topología actualizada: {} nodos",
                    self.node_id,
                    nodes.len()
                ));
                for node in &nodes {
                    let marker = if node.id == self.node_id {
                        " \x1b[32m(yo)\x1b[0m"
                    } else {
                        ""
                    };
                    log_info(&format!(
                        "[Ring #{}]    • Nodo #{}: {}{}",
                        self.node_id, node.id, node.addr, marker
                    ));
                }
            }

            if leader_changed {
                log_info(&format!(
                    "[Ring #{}] [INFO] Nuevo líder: Nodo #{}",
                    self.node_id, leader_id
                ));
            }
        }

        // Actualizar información del líder
        self.current_leader_id.store(leader_id, Ordering::SeqCst);
        self.heartbeat_monitor.update();

        // Propagar ClusterSync con la lista actualizada (que ahora me incluye)
        let cluster_msg: Vec<NodeInfoMsg> = nodes.iter().map(|n| n.into()).collect();
        let sync_msg = ElectionMessage::ClusterSync {
            cluster_nodes: cluster_msg,
            leader_id,
            origin_id, // Mantener el ID del origen
            client_addr: self.client_addr.to_string(),
        };
        let _ = self.send_to_successor(&sync_msg);
    }

    fn handle_state_update(
        &self,
        operation_type: u8,
        account_id: u64,
        data: Vec<u8>,
        timestamp: u64,
    ) {
        // El ring SOLO recibe StateUpdate de otros nodos para aplicar localmente
        // NO debe propagarlo - el Thread 3 (replicación) maneja toda la propagación
        if !self.is_leader.load(Ordering::SeqCst)
            && let Some(ref sender) = self.state_update_sender
        {
            let update = StateUpdateData {
                operation_type,
                account_id,
                data,
                timestamp,
            };
            let _ = sender.send(update);
        }
        // NO propagar - esto lo hace el Thread 3 con sus propias conexiones
    }

    /// Envía un cambio de estado al canal para que el Thread 3 lo procese
    /// El ring NO maneja replicación de datos, solo pone en el canal
    pub fn replicate_state_async(&self, update: StateUpdateData) {
        // Solo el líder envía cambios nuevos al canal
        // Los followers reciben cambios del Thread 3 (no del ring)
        if !self.is_leader.load(Ordering::SeqCst) {
            return;
        }

        // Enviar a través del canal al Thread 3 (fire-and-forget, no bloquea)
        if let Some(ref sender) = self.state_update_sender {
            // Si el canal está lleno o cerrado, se descarta silenciosamente
            let _ = sender.send(update);
        }
    }

    /// Envía la topología actual del cluster a todos los nodos en el anillo.
    /// Usado cuando el líder detecta que un nodo se recuperó.
    fn broadcast_cluster_sync(&self) {
        let nodes = self.cluster_nodes.read().unwrap();

        // Solo enviar si tenemos una lista completa (más de 1 nodo)
        if nodes.len() <= 1 {
            return;
        }

        let cluster_msg: Vec<NodeInfoMsg> = nodes.iter().map(|n| n.into()).collect();
        let leader_id = self.current_leader_id.load(Ordering::SeqCst);
        drop(nodes);

        // No loguear - se hace cada 15 segundos y es rutinario
        // Solo los nodos detectarán cambios si los hay

        let sync_msg = ElectionMessage::ClusterSync {
            cluster_nodes: cluster_msg,
            leader_id,
            origin_id: self.node_id, // Yo soy el origen de este sync,
            client_addr: self.client_addr.to_string(),
        };

        // Enviar a través del anillo (se propagará a todos)
        let _ = self.send_to_successor(&sync_msg);
    }

    /// Obtiene la dirección de cliente de un nodo específico del cluster.
    /// Convierte el ring_port a client_port usando la relación: client_port = ring_port - 1292
    /// (8081 -> 6789, 8082 -> 6790, 8083 -> 6791, 8084 -> 6792)
    pub fn get_node_client_address(&self, node_id: u8) -> Option<String> {
        let nodes = self.cluster_nodes.read().unwrap();
        nodes.iter().find(|n| n.id == node_id).map(|n| {
            if !n.client_addr.is_empty() {
                return n.client_addr.clone();
            }
            // Fallback: derivar del puerto del anillo si no hay client_addr
            if let Some(port_str) = n.addr.split(':').nth(1)
                && let Ok(ring_port) = port_str.parse::<u16>()
            {
                let client_port = ring_port.saturating_sub(1292);
                if let Some(ip) = n.addr.split(':').next() {
                    return format!("{}:{}", ip, client_port);
                }
            }
            "127.0.0.1:6789".to_string()
        })
    }

    /// Crea una copia mínima para pasar a threads.
    fn clone_minimal(&self) -> ClonableRingNode {
        ClonableRingNode {
            node_id: self.node_id,
            is_leader: Arc::clone(&self.is_leader),
            current_leader_id: Arc::clone(&self.current_leader_id),
            successor_addr: self.successor_addr.clone(),
            cluster_nodes: Arc::clone(&self.cluster_nodes),
            ring_port: self.ring_port,
            is_single_node_ring: self.is_single_node_ring,
            client_addr: self.client_addr.clone(),
            heartbeat_monitor: Arc::clone(&self.heartbeat_monitor),
            heartbeat_sender: Arc::clone(&self.heartbeat_sender),
            running: Arc::clone(&self.running),
            election_in_progress: Arc::clone(&self.election_in_progress),
            state_update_sender: self.state_update_sender.clone(),
            heartbeat_error_count: Arc::clone(&self.heartbeat_error_count),
            last_heartbeat_seq: Arc::clone(&self.last_heartbeat_seq),
            primary_successor_failed: Arc::clone(&self.primary_successor_failed),
            logged_alternative_use: Arc::clone(&self.logged_alternative_use),
        }
    }
}

/// Estructura auxiliar para clonar datos del nodo entre threads.
struct ClonableRingNode {
    node_id: u8,
    is_leader: Arc<AtomicBool>,
    current_leader_id: Arc<AtomicU8>,
    successor_addr: String,
    cluster_nodes: Arc<RwLock<Vec<NodeInfo>>>,
    ring_port: u16,
    is_single_node_ring: bool,
    client_addr: String,
    heartbeat_monitor: Arc<HeartbeatMonitor>,
    heartbeat_sender: Arc<HeartbeatSender>,
    running: Arc<AtomicBool>,
    election_in_progress: Arc<AtomicBool>,
    state_update_sender: Option<Sender<StateUpdateData>>,
    heartbeat_error_count: Arc<AtomicU64>,
    last_heartbeat_seq: Arc<Mutex<u64>>,
    primary_successor_failed: Arc<AtomicBool>,
    logged_alternative_use: Arc<AtomicBool>,
}
