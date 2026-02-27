mod pump;
use pump::Pump;
use shared_resources::utils::get_timestamp;
use std::env;
use std::net::ToSocketAddrs;
use std::process;
use std::thread;
use std::time::Duration;

/// Log informativo con timestamp y componente
fn log_info(port: u16, msg: &str) {
    println!("[{}] [PUMP #{}] [INFO] {}", get_timestamp(), port, msg);
}

/// Log de error con timestamp y componente
fn log_error(port: u16, msg: &str) {
    eprintln!("[{}] [PUMP #{}] [ERROR] {}", get_timestamp(), port, msg);
}

fn main() {
    let args: Vec<String> = env::args().collect();
    // Argumentos: programa --station-addr <ip:port> --response-port <port> --local-addr <ip> [--mode <api|test>] [--api-port <port>]
    if args.len() < 7 {
        eprintln!("Uso: {} --station-addr <ip:port> --response-port <port> --local-addr <ip> [--mode <api|test>] [--api-port <port>]", args[0]);
        eprintln!("Ejemplo API:  {} --station-addr 127.0.0.1:8080 --response-port 5001 --local-addr 127.0.0.1 --mode api --api-port 7080", args[0]);
        eprintln!("Ejemplo TEST: {} --station-addr 127.0.0.1:8080 --response-port 5001 --local-addr 127.0.0.1 --mode test", args[0]);
        eprintln!();
        eprintln!("Modos:");
        eprintln!(
            "  api  - Inicia servidor API TCP para recibir transacciones en JSON (por defecto)"
        );
        eprintln!("  test - Envía 5 transacciones de prueba hardcodeadas y termina");
        process::exit(1);
    }

    let station_address = parse_arg_string(&args, "--station-addr").expect("Falta --station-addr");
    let response_port: u16 = parse_arg(&args, "--response-port").expect("Falta --response-port");
    let local_addr = parse_arg_string(&args, "--local-addr").expect("Falta --local-addr");
    let mode = parse_arg_string(&args, "--mode").unwrap_or_else(|| "api".to_string());
    let api_port: u16 = parse_arg(&args, "--api-port").unwrap_or(7080);

    // Validate that the station address has a valid format (IP:PORT)
    if station_address.to_socket_addrs().is_err() {
        eprintln!("[PUMP #{}] [ERROR] Invalid station address. Ensure it is in the format IP:PORT (e.g., 127.0.0.1:8080).", response_port);
        return;
    }

    let pump_full_address = format!("{}:{}", local_addr, response_port);

    log_info(response_port, "Iniciando Pump");
    log_info(
        response_port,
        &format!("Conectando a GasStation en {}", station_address),
    );
    log_info(
        response_port,
        &format!("Escuchando respuestas en {}", pump_full_address),
    );

    let pump = Pump::new(response_port, pump_full_address.clone());

    // Iniciar listener de respuestas en background
    let _response_listener = pump.start_response_listener();

    match mode.as_str() {
        "api" => run_api_mode(api_port, station_address, response_port, pump_full_address),
        "test" => run_test_mode(station_address, response_port, pump_full_address),
        _ => {
            log_error(
                response_port,
                &format!("Modo desconocido: '{}'. Use 'api' o 'test'", mode),
            );
            process::exit(1);
        }
    }
}

fn run_api_mode(api_port: u16, station_address: String, response_port: u16, pump_address: String) {
    log_info(response_port, "MODO API - Esperando transacciones vía TCP");
    // Iniciar API TCP
    Pump::start_transaction_api(api_port, station_address, response_port, pump_address);

    log_info(response_port, "");
    log_info(response_port, "Como enviar transacciones:");
    log_info(response_port, &format!("   echo '{{\"account_id\": 123, \"card_id\": 456, \"amount\": 500.0}}' | nc localhost {}", api_port));
    log_info(response_port, "");
    log_info(response_port, "Presiona Ctrl+C para detener el pump");
    log_info(response_port, "");

    // Mantener el programa vivo indefinidamente
    loop {
        thread::sleep(Duration::from_secs(3600));
    }
}

fn run_test_mode(station_address: String, response_port: u16, pump_address: String) {
    let mut pump = Pump::new(response_port, pump_address);

    log_info(
        response_port,
        "MODO TEST - Enviando 5 transacciones hardcodeadas",
    );
    log_info(response_port, "");

    // Transacciones de prueba (5 en total, enviadas rápidamente para que vayan en el mismo bulk)
    let test_transactions = [
        (
            1,
            5001,
            100.0,
            "TX 1: Válida - cuenta 1, tarjeta 5001, $100",
        ),
        (
            1,
            5001,
            200.0,
            "TX 2: Válida - cuenta 1, tarjeta 5001, $200",
        ),
        (
            1,
            5001,
            150.0,
            "TX 3: Válida - cuenta 1, tarjeta 5001, $150",
        ),
        (
            1,
            5001,
            15000.0,
            "TX 4: INVÁLIDA - excede límite de tarjeta (límite: $10,000)",
        ),
        (
            1,
            5002,
            300.0,
            "TX 5: Válida - cuenta 1, tarjeta 5002, $300",
        ),
    ];

    for (i, (account_id, card_id, amount, description)) in test_transactions.iter().enumerate() {
        log_info(
            response_port,
            &format!("[{}] Enviando: {}", i + 1, description),
        );
        let transaction = pump.generate_transaction(*account_id, *card_id, *amount, 1);

        match pump.send_transaction(&transaction, &station_address) {
            Ok(_) => log_info(
                response_port,
                &format!("Enviada con ID: {}", transaction.transaction_id),
            ),
            Err(e) => log_error(response_port, &format!("Error enviando: {}", e)),
        }

        // Pequeña pausa para evitar congestión pero lo suficientemente rápido para el bulk
        thread::sleep(Duration::from_millis(100));
    }

    log_info(response_port, "");
    log_info(response_port, "TODAS LAS TRANSACCIONES ENVIADAS");
    log_info(response_port, "Esperando respuestas de la Gas Station...");

    // Mantener el programa vivo para recibir respuestas
    // (las respuestas son asíncronas y pueden tardar varios segundos)
    thread::sleep(Duration::from_secs(30));

    log_info(response_port, "FIN DE LA PRUEBA");
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
