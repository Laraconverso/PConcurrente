use std::io::{self, Write};
use std::net::TcpStream;
use std::sync::mpsc::Sender;

use crate::server::accounts::AccountsManager;
use shared_resources::messages::protocol::RegionalProtocolMessage;
use shared_resources::messages::transaction::TransactionMessage;
use shared_resources::utils::get_timestamp;

/// Log informativo con timestamp y componente
fn log_info(regional_id: u8, msg: &str) {
    println!(
        "[{}] [REGIONAL #{}] [INFO] {}",
        get_timestamp(),
        regional_id,
        msg
    );
}

/// Log de error con timestamp y componente
fn log_error(regional_id: u8, msg: &str) {
    eprintln!(
        "[{}] [REGIONAL #{}] [ERROR] {}",
        get_timestamp(),
        regional_id,
        msg
    );
}

pub struct ClientHandler {
    stream: TcpStream,
    accounts: AccountsManager,
    peer_addr: String,
    central_sender: Sender<Vec<TransactionMessage>>,
    regional_id: u8,
}

impl ClientHandler {
    pub fn new(
        stream: TcpStream,
        accounts: AccountsManager,
        central_sender: Sender<Vec<TransactionMessage>>,
        regional_id: u8,
    ) -> Self {
        let peer_addr = stream
            .peer_addr()
            .map(|a| a.to_string())
            .unwrap_or_else(|_| "Unknown".to_string());

        Self {
            stream,
            accounts,
            peer_addr,
            central_sender,
            regional_id,
        }
    }

    pub fn run(&mut self) {
        log_info(
            self.regional_id,
            &format!("ClientHandler - Conectado a {}", self.peer_addr),
        );

        let stream_clone = self.stream.try_clone().expect("error clonando stream");
        let deserializer = serde_json::Deserializer::from_reader(stream_clone)
            .into_iter::<RegionalProtocolMessage>();

        for msg_result in deserializer {
            match msg_result {
                Ok(msg) => {
                    if let Err(e) = self.process_message(msg) {
                        log_error(
                            self.regional_id,
                            &format!("ClientHandler - Error procesando mensaje: {}", e),
                        );
                        break;
                    }
                }
                Err(e) => {
                    log_error(
                        self.regional_id,
                        &format!("ClientHandler - Error de conexión o deserialización: {}", e),
                    );
                    break;
                }
            }
        }

        log_info(
            self.regional_id,
            &format!("ClientHandler - Conexión finalizada con {}", self.peer_addr),
        );
    }

    fn process_message(&mut self, msg: RegionalProtocolMessage) -> io::Result<()> {
        match msg {
            RegionalProtocolMessage::BulkTransaction {
                bulk_id,
                transactions,
            } => {
                log_info(
                    self.regional_id,
                    &format!(
                        "2PC PREPARE - Lote: {}, cantidad: {}",
                        bulk_id,
                        transactions.len()
                    ),
                );

                let (vote, accepted, rejected) =
                    self.accounts
                        .reserve_batch(bulk_id, transactions, self.regional_id);

                if vote {
                    log_info(
                        self.regional_id,
                        &format!(
                            "2PC VOTE_COMMIT - {} aceptadas, {} rechazadas",
                            accepted.len(),
                            rejected.len()
                        ),
                    );
                } else {
                    log_info(
                        self.regional_id,
                        "2PC VOTE_ABORT - Todas las transacciones rechazadas",
                    );
                }

                let response = RegionalProtocolMessage::BulkPrepareResponse {
                    bulk_id,
                    vote,
                    accepted_transactions: accepted,
                    rejected_transactions: rejected,
                };
                self.send(response)?;
            }

            RegionalProtocolMessage::GlobalCommit { bulk_id, .. } => {
                log_info(self.regional_id, &format!("2PC COMMIT - Lote: {}", bulk_id));

                if let Some(transactions) = self.accounts.commit_batch(bulk_id, self.regional_id) {
                    self.central_sender
                        .send(transactions)
                        .map_err(|e| io::Error::new(io::ErrorKind::BrokenPipe, e.to_string()))?;

                    let ack = RegionalProtocolMessage::BulkCommittedAck { bulk_id };
                    self.send(ack)?;
                } else {
                    log_error(
                        self.regional_id,
                        &format!(
                            "2PC - Error: intento de COMMIT de lote desconocido o expirado: {}",
                            bulk_id
                        ),
                    );
                }
            }

            RegionalProtocolMessage::GlobalAbort { bulk_id, .. } => {
                log_info(self.regional_id, &format!("2PC ABORT - Lote: {}", bulk_id));
                self.accounts.rollback_batch(bulk_id, self.regional_id);
            }

            _ => {
                log_error(self.regional_id, "ClientHandler - Mensaje inesperado");
            }
        }
        Ok(())
    }

    fn send(&mut self, msg: RegionalProtocolMessage) -> io::Result<()> {
        let json = serde_json::to_string(&msg)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

        self.stream.write_all(json.as_bytes())?;
        self.stream.write_all(b"\n")?;
        self.stream.flush()?;
        Ok(())
    }
}
