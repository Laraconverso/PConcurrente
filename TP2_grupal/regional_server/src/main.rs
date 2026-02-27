mod server;
mod types;

use crate::server::RegionalServer;
use std::env;
use std::process;

fn main() {
    let args: Vec<String> = env::args().collect();

    // Argumentos con flags: --name <nombre> --port <num> --sync-port <num> --central <ip:port> [--central <ip:port> ...]
    if args.len() < 7 {
        eprintln!(
            "Uso: {} --name <nombre_region> --port <listen_port> --sync-port <central_sync_port> --central <ip:port> [--central <ip:port> ...]",
            args[0]
        );
        eprintln!(
            "Ejemplo: {} --name CABA --port 8002 --sync-port 8012 --central 127.0.0.1:9000 --central 127.0.0.1:9001",
            args[0]
        );
        eprintln!("Nota: El nombre debe ser alfanumérico (ej: CABA, Buenos_Aires, Region-1)");
        process::exit(1);
    }

    let regional_name: String = parse_arg_string(&args, "--name").unwrap_or_else(|| {
        eprintln!("Falta --name (nombre de la región, debe ser alfanumérico)");
        process::exit(1);
    });

    let port: u16 = parse_arg(&args, "--port").unwrap_or_else(|| {
        eprintln!("Falta o es inválido --port");
        process::exit(1);
    });

    let sync_port: u16 = parse_arg(&args, "--sync-port").unwrap_or_else(|| {
        eprintln!("Falta o es inválido --sync-port");
        process::exit(1);
    });

    // Recolectar todas las direcciones centrales (puede haber múltiples --central)
    let central_addresses = parse_multiple_args(&args, "--central");
    if central_addresses.is_empty() {
        eprintln!(
            "[Regional '{}'] Error: Debe especificar al menos una dirección central con --central",
            regional_name
        );
        process::exit(1);
    }

    println!("[Regional '{}'] Iniciando Servidor Regional", regional_name);
    println!(
        "[Regional '{}'] Configuración - Nombre: {}",
        regional_name, regional_name
    );
    println!(
        "[Regional '{}'] Configuración - Puerto: {}",
        regional_name, port
    );
    println!(
        "[Regional '{}'] Configuración - Puerto Sync: {}",
        regional_name, sync_port
    );
    println!(
        "[Regional '{}'] Configuración - Réplicas Central: {:?}",
        regional_name, central_addresses
    );

    let mut server = RegionalServer::new(regional_name.clone(), port, sync_port, central_addresses);

    // Registrarse con el Central Server
    println!(
        "[Regional '{}'] Registrándose con el Central Server...",
        regional_name
    );
    let (region_id, initial_accounts) = match server.register_with_central() {
        Ok(result) => result,
        Err(e) => {
            eprintln!(
                "[Regional '{}'] Error fatal registrándose: {}",
                regional_name, e
            );
            process::exit(1);
        }
    };

    println!(
        "[Regional '{}'] Registrado con ID {}",
        regional_name, region_id
    );
    println!(
        "[Regional '{}'] Recibidas {} cuentas iniciales",
        regional_name,
        initial_accounts.len()
    );

    // Cargar cuentas recibidas del Central
    for account in initial_accounts {
        server.load_account_from_sync(account);
    }

    println!("[Regional '{}'] Cuentas cargadas en caché", regional_name);

    // Iniciar listener de sincronización del Central (en background)
    if let Err(e) = server.start_central_sync_listener() {
        eprintln!(
            "[Regional '{}'] Error iniciando listener de sync: {}",
            regional_name, e
        );
        process::exit(1);
    }

    if let Err(e) = server.run() {
        eprintln!(
            "[Regional '{}'] Error fatal en el servidor: {}",
            regional_name, e
        );
        process::exit(1);
    }
}

/// Parsea un argumento numérico de la línea de comandos.
fn parse_arg<T: std::str::FromStr>(args: &[String], flag: &str) -> Option<T> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1))
        .and_then(|s| s.parse().ok())
}

/// Parsea un argumento string de la línea de comandos.
fn parse_arg_string(args: &[String], flag: &str) -> Option<String> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1))
        .cloned()
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
