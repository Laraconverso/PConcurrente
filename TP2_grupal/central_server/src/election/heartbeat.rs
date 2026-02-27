//! Módulo para gestionar heartbeats y detección de fallos del líder.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Gestor de heartbeats que monitorea la salud del líder.
pub struct HeartbeatMonitor {
    /// Timestamp del último heartbeat recibido.
    last_heartbeat: Arc<Mutex<Instant>>,
    /// Indica si el monitor debe continuar ejecutándose.
    running: Arc<AtomicBool>,
    /// Timeout en segundos para considerar que el líder falló.
    timeout_secs: u64,
}

impl HeartbeatMonitor {
    /// Crea un nuevo monitor de heartbeats.
    pub fn new(timeout_secs: u64) -> Self {
        Self {
            last_heartbeat: Arc::new(Mutex::new(Instant::now())),
            running: Arc::new(AtomicBool::new(true)),
            timeout_secs,
        }
    }

    /// Actualiza el timestamp del último heartbeat recibido.
    pub fn update(&self) {
        *self.last_heartbeat.lock().unwrap() = Instant::now();
    }

    /// Retorna el tiempo transcurrido desde el último heartbeat.
    pub fn elapsed_since_last_heartbeat(&self) -> Duration {
        self.last_heartbeat.lock().unwrap().elapsed()
    }

    /// Verifica si el líder ha fallado (timeout excedido).
    pub fn is_timeout(&self) -> bool {
        self.elapsed_since_last_heartbeat() > Duration::from_secs(self.timeout_secs)
    }

    /// Verifica si el monitor está corriendo.
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }
}

/// Gestor para envío periódico de heartbeats (usado por el líder).
pub struct HeartbeatSender {
    sequence: Arc<AtomicU64>,
    interval_secs: u64,
}

impl HeartbeatSender {
    /// Crea un nuevo emisor de heartbeats.
    pub fn new(interval_secs: u64) -> Self {
        Self {
            sequence: Arc::new(AtomicU64::new(0)),
            interval_secs,
        }
    }

    /// Obtiene el siguiente número de secuencia.
    pub fn next_sequence(&self) -> u64 {
        self.sequence.fetch_add(1, Ordering::SeqCst)
    }

    /// Retorna el intervalo de envío de heartbeats.
    pub fn interval(&self) -> Duration {
        Duration::from_secs(self.interval_secs)
    }
}
