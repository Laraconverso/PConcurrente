use crate::types::SyncPair;
use shared_resources::messages::account::register_account::REG_ACCOUNT;
use shared_resources::messages::data_response::DataResponseMessage;
use shared_resources::messages::probe::probe_response::{PROBE, ProbeResponse};
use shared_resources::utils::{FAILURE, SUCCESS, write_and_flush};
use std::error::Error;
use std::io::{Read, Write, stdout};
use std::net::TcpStream;
use std::sync::MutexGuard;
use std::sync::mpsc::{Sender, channel};
use std::thread;
use std::time::Duration;

pub enum ResponseResult {
    Success,
    LeaderRedirect(String),
    Error(String),
}

pub struct Messenger {
    central_stream: TcpStream,
    known_nodes: Vec<String>,
}

impl Messenger {
    pub fn new(ips: Vec<String>) -> Option<Self> {
        for ip in ips {
            let central_data = probe_node(ip.clone());
            if central_data.is_err() {
                continue;
            }
            let (success_ip, nodes) = central_data.unwrap();
            let central_stream = connect_to_leader(success_ip);
            if central_stream.is_none() {
                continue;
            }
            return Some(Self {
                central_stream: central_stream.unwrap(),
                known_nodes: nodes,
            });
        }
        None
    }

    pub fn start_listening(&mut self, pair: SyncPair) -> Sender<Vec<u8>> {
        let (tx, rx): (Sender<Vec<u8>>, _) = channel();

        let mut active_stream = self.central_stream.try_clone().unwrap();
        let nodes_list = self.known_nodes.clone();
        thread::spawn(move || {
            let (lock, cvar) = &*pair;

            for serialized_message in rx {
                let pending = serialized_message;
                loop {
                    match active_stream.write_all(&pending) {
                        Ok(()) => {
                            if let Err(e) = active_stream.flush() {
                                println!("Flush failed (likely disconnect): {}", e);
                                if let Some(new_stream) = reconnect_via_probe(&nodes_list) {
                                    active_stream = new_stream;
                                    continue;
                                } else {
                                    println!("No leader found. Will retry in 500ms...");
                                    thread::sleep(Duration::from_millis(500));
                                    continue;
                                }
                            }

                            thread::sleep(Duration::from_millis(100));
                            let got_res = lock.lock().unwrap();
                            match wait_and_display(&active_stream, got_res) {
                                ResponseResult::Success => {
                                    cvar.notify_one();
                                    break;
                                }
                                ResponseResult::LeaderRedirect(leader_addr) => {
                                    println!("Conectando al líder en {}...", leader_addr);
                                    match TcpStream::connect(&leader_addr) {
                                        Ok(new_stream) => {
                                            println!(
                                                "Conectado al líder, reintentando operación..."
                                            );
                                            active_stream = new_stream;
                                            continue; // resend pending
                                        }
                                        Err(e) => {
                                            println!(
                                                "Error conectando al líder {}: {}",
                                                leader_addr, e
                                            );
                                            // Intentar con probe como fallback
                                            if let Some(new_stream) =
                                                reconnect_via_probe(&nodes_list)
                                            {
                                                println!("Encontrado líder alternativo");
                                                active_stream = new_stream;
                                                continue;
                                            } else {
                                                println!("No leader found. Will retry in 500ms...");
                                                thread::sleep(Duration::from_millis(500));
                                                continue;
                                            }
                                        }
                                    }
                                }
                                ResponseResult::Error(e) => {
                                    println!("Error receiving response: {}", e);
                                    if let Some(new_stream) = reconnect_via_probe(&nodes_list) {
                                        println!("Encontrado nuevo líder");
                                        active_stream = new_stream;
                                        continue; // resend pending
                                    } else {
                                        println!("No leader found. Will retry in 500ms...");
                                        thread::sleep(Duration::from_millis(500));
                                        continue;
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            println!("Error when sending data: {}", e);
                            if let Some(new_stream) = reconnect_via_probe(&nodes_list) {
                                active_stream = new_stream;
                                continue; // retry send
                            } else {
                                println!("No leader found. Will retry in 500ms...");
                                thread::sleep(Duration::from_millis(500));
                                continue;
                            }
                        }
                    }
                }
            }
        });
        tx
    }
}

fn probe_node(ip: String) -> Result<(String, Vec<String>), Box<dyn Error>> {
    let aux_ip = ip.clone();
    if let Ok(mut stream) = TcpStream::connect(aux_ip) {
        let mut buf = Vec::new();
        let len = 1u64;
        buf.extend_from_slice(&len.to_be_bytes());
        buf.push(PROBE);

        if write_and_flush(&mut stream, buf).is_err() {
            println!("Error when connecting to {ip}");
            return Err(Box::from("Socket not connected"));
        }
        thread::sleep(Duration::from_millis(100));
        // Escucho el stream por la respuesta
        let mut len_res_buf = [0u8; 8];
        stream.read_exact(&mut len_res_buf)?;
        thread::sleep(Duration::from_millis(100));
        let total_len = u64::from_be_bytes(len_res_buf) as usize;
        let mut res_buf = vec![0u8; total_len];
        thread::sleep(Duration::from_millis(100));
        stream.read_exact(&mut res_buf)?;

        let aux_msg = DataResponseMessage::deserialize(&res_buf)?;
        let payload = aux_msg.piggyback().unwrap();
        let res_msg = ProbeResponse::deserialize(&payload)?;
        return Ok((res_msg.ip(), res_msg.nodes()));
    }
    Err(Box::from("Unreachable IP"))
}

fn connect_to_leader(ip: String) -> Option<TcpStream> {
    if let Ok(stream) = TcpStream::connect(ip) {
        println!("Central node connected");
        return Some(stream);
    }
    println!("Error connecting to the central node");
    None
}

/// Try to discover and connect to a current leader by probing known nodes.
/// Returns a fresh `TcpStream` on success, or `None` if no leader could be reached.
fn reconnect_via_probe(nodes: &[String]) -> Option<TcpStream> {
    for node_ip in nodes {
        match probe_node(node_ip.clone()) {
            Ok((new_leader_ip, _)) => match TcpStream::connect(&new_leader_ip) {
                Ok(new_stream) => {
                    println!("New leader found: {}", new_leader_ip);
                    return Some(new_stream);
                }
                Err(e) => {
                    println!(
                        "Error when connecting to the new leader {}: {}",
                        new_leader_ip, e
                    );
                }
            },
            Err(e) => {
                println!("Node {} failed the probing process: {}", node_ip, e);
            }
        }
    }
    None
}

pub fn wait_and_display(
    mut stream: &TcpStream,
    mut id: MutexGuard<(bool, Option<u64>)>,
) -> ResponseResult {
    thread::sleep(Duration::from_millis(500));
    let mut total_len_bytes = [0u8; 8];
    if let Err(e) = stream.read_exact(&mut total_len_bytes) {
        return ResponseResult::Error(format!("Error reading response size: {}", e));
    }
    let total_len = u64::from_be_bytes(total_len_bytes) as usize;

    let mut buf = vec![0u8; total_len];
    if let Err(e) = stream.read_exact(&mut buf) {
        return ResponseResult::Error(format!("Error reading response payload: {}", e));
    }

    let msg = match DataResponseMessage::deserialize(&buf) {
        Ok(m) => m,
        Err(e) => return ResponseResult::Error(format!("Error deserializing response: {}", e)),
    };

    // Verificar si es una redirección (FAILURE con dirección del líder en piggyback)
    if msg.status() == FAILURE
        && let Some(leader_bytes) = msg.piggyback()
    {
        // Intentar interpretar como dirección del líder (UTF-8)
        if let Ok(leader_addr) = String::from_utf8(leader_bytes) {
            // Si parece una dirección IP:puerto, es una redirección
            if leader_addr.contains(':') && !leader_addr.contains('\n') {
                println!("Redirección detectada al líder: {}", leader_addr);
                id.0 = false; // No marcar como completado, vamos a reintentar
                return ResponseResult::LeaderRedirect(leader_addr);
            }
        }
    }

    msg.display_response();

    if msg.code() == REG_ACCOUNT && msg.status() == SUCCESS {
        id.1 = Some(msg.id());
    }
    id.0 = true;
    let _ = stdout().flush();
    ResponseResult::Success
}
