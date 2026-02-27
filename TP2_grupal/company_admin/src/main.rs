use crate::admin::CompanyAdministrator;
use crate::messenger::Messenger;
use chrono::Local;
use std::env;
use std::process;
use std::sync::{Arc, Condvar, Mutex};

mod admin;
mod commands;
mod messenger;
mod types;

/// Log de error con timestamp y componente
fn log_error_simple(msg: &str) {
    eprintln!(
        "[{}] [ADMIN] [ERROR] {}",
        Local::now().format("%H:%M:%S%.3f"),
        msg
    );
}

/// Log informativo con timestamp y componente
fn log_info(msg: &str) {
    println!(
        "[{}] [ADMIN] [INFO] {}",
        Local::now().format("%H:%M:%S%.3f"),
        msg
    );
}

fn main() {
    let args: Vec<String> = env::args().collect();

    // Argumentos con flags: --central <ip:port> [--central <ip:port> ...]
    if args.len() < 3 {
        eprintln!(
            "Uso: {} --central <ip:port> [--central <ip:port> ...]",
            args[0]
        );
        eprintln!(
            "Ejemplo: {} --central 127.0.0.1:9000 --central 127.0.0.1:9001",
            args[0]
        );
        eprintln!("Nota: Puede especificar múltiples réplicas del Central Server");
        process::exit(1);
    }

    // Recolectar todas las direcciones centrales (puede haber múltiples --central)
    let central_addresses = parse_multiple_args(&args, "--central");
    if central_addresses.is_empty() {
        log_error_simple("Error: Debe especificar al menos una dirección central con --central");
        log_error_simple("Ejemplo: --central 127.0.0.1:9000");
        process::exit(1);
    }

    log_info("Iniciando Company Administrator");
    log_info(&format!(
        "Configuración - Réplicas Central: {:?}",
        central_addresses
    ));

    if let Some(mut messenger) = Messenger::new(central_addresses) {
        let pair = Arc::new((Mutex::new((false, None)), Condvar::new()));
        let pair_cloned = pair.clone();

        let output_sender = messenger.start_listening(pair_cloned);

        let mut admin = CompanyAdministrator::new(output_sender);
        admin.run(pair);
    } else {
        log_error_simple("No se pudo conectar a ningún servidor central");
        println!("Shutting down");
        process::exit(1);
    }
}

/// Parsea múltiples ocurrencias de un flag (ej: --central puede aparecer varias veces)
fn parse_multiple_args(args: &[String], flag: &str) -> Vec<String> {
    let mut results = Vec::new();
    let mut i = 0;

    while i < args.len() {
        if args[i] == flag
            && let Some(value) = args.get(i + 1)
            && !value.starts_with("--")
        {
            results.push(value.clone());
        }
        i += 1;
    }

    results
}
