mod gas_station;
mod types;

use gas_station::{GasStation, GasStationConfig}; // Importamos GasStation y GasStationConfig
use shared_resources::utils::get_timestamp;
use std::env;
use std::process;

/// Log informativo con timestamp y componente
fn log_info(station_id: u16, msg: &str) {
    println!(
        "[{}] [STATION #{}] [INFO] {}",
        get_timestamp(),
        station_id,
        msg
    );
}

fn main() {
    let args: Vec<String> = env::args().collect();

    // --- 1. Definición de Argumentos Mínimos Requeridos ---
    // Total de 11 argumentos: program + 5 flags * 2 valores
    // Flags requeridas: --pump-port, --regional-addr, --regional-send-port, --bulk-limit, --bulk-timeout
    if args.len() < 11 {
        eprintln!(
            "Uso: {} --pump-port <port> --regional-addr <ip> --regional-send-port <port> --bulk-limit <num> --bulk-timeout <secs>",
            args[0]
        );
        eprintln!(
            "Ejemplo: {} --pump-port 8080 --regional-addr 127.0.0.1 --regional-send-port 9000 --bulk-limit 15 --bulk-timeout 45",
            args[0]
        );
        process::exit(1);
    }

    // --- 2. Extracción y Parseo de Argumentos ---

    // Parámetros de Red
    let pump_listener_port: u16 = parse_arg(&args, "--pump-port").unwrap_or_else(|| {
        eprintln!("Falta o es inválido --pump-port");
        process::exit(1);
    });
    let regional_address = parse_arg_string(&args, "--regional-addr").unwrap_or_else(|| {
        eprintln!("Falta o es inválido --regional-addr");
        process::exit(1);
    });
    let regional_send_port: u16 = parse_arg(&args, "--regional-send-port").unwrap_or_else(|| {
        eprintln!("Falta o es inválido --regional-send-port");
        process::exit(1);
    });

    // Parámetros de Lógica (Bulk)
    let bulk_limit: usize = parse_arg(&args, "--bulk-limit").unwrap_or_else(|| {
        eprintln!("Falta o es inválido --bulk-limit");
        process::exit(1);
    });
    let bulk_timeout_secs: u64 = parse_arg(&args, "--bulk-timeout").unwrap_or_else(|| {
        eprintln!("Falta o es inválido --bulk-timeout");
        process::exit(1);
    });

    // --- 3. Creación del Objeto de Configuración ---
    let config = GasStationConfig {
        pump_listener_port,
        regional_address,
        regional_port: regional_send_port, // Usamos 'regional_port' en el struct, alimentado por --regional-send-port
        bulk_limit,
        bulk_timeout_secs,
    };

    log_info(pump_listener_port, "Iniciando GasStation");
    log_info(
        pump_listener_port,
        &format!(
            "Configuración - Pumps Listener: puerto {}",
            config.pump_listener_port
        ),
    );
    log_info(
        pump_listener_port,
        &format!(
            "Configuración - Regional Send: {}:{}",
            config.regional_address, config.regional_port
        ),
    );
    log_info(
        pump_listener_port,
        &format!(
            "Configuración - Bulk Limit: {}, Timeout: {}s",
            config.bulk_limit, config.bulk_timeout_secs
        ),
    );

    // --- 4. Inicialización y Ejecución ---
    let gas_station = GasStation::new(config);
    let handle = gas_station.start();

    // Bloqueamos el hilo principal para que el programa no termine
    handle.join().expect("GasStation thread panicked");
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
