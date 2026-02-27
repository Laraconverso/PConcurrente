use chrono::{Local, NaiveDate, Utc};
use shared_resources::messages::account::register_account::RegisterAccountMessage;
use shared_resources::messages::card::register_card::RegisterCardMessage;
use shared_resources::messages::query::query_data::QueryDataMessage;
use shared_resources::messages::remove_regions::RemoveRegionsMessage;
use shared_resources::messages::update_data::UpdateDataMessage;
use shared_resources::messages::update_regions::UpdateRegionsMessage;
use shared_resources::utils::{AccountId, CardId, TimeStamp, frame_payload};
use std::str::FromStr;
use std::sync::mpsc::Sender;

const COMMAND: usize = 0;

/// Log informativo con timestamp
fn log_info(msg: &str) {
    println!(
        "[{}] [ADMIN] [INFO] {}",
        Local::now().format("%H:%M:%S%.3f"),
        msg
    );
}

/// Log de error con timestamp
fn log_error(msg: &str) {
    eprintln!(
        "[{}] [ADMIN] [ERROR] {}",
        Local::now().format("%H:%M:%S%.3f"),
        msg
    );
}

#[derive(Debug)]
pub enum Command {
    LogIn(AccountId),
    LogOut(),
    RegAcc(String, String), // username, display_name (SIN región ni límite)
    RegCard(f64),
    Upd(String, CardId, f64),
    UpdRegions(Vec<String>, Vec<f64>), // Agregar/actualizar regiones con sus límites
    RemoveRegions(Vec<String>),        // Remover regiones
    //Del(CardId),
    Query(u8, CardId, TimeStamp, TimeStamp),
    Help(),
    Null(),
    Exit(),
}

impl Command {
    pub fn parse(cmd: String) -> Self {
        let tokens = parse_command_with_quotes(cmd.trim());
        if tokens.is_empty() {
            return Self::Null();
        }

        // Hacer el comando case-insensitive
        let command = tokens[COMMAND].to_uppercase();
        match command.as_str() {
            "LOGIN" => {
                if tokens.len() < 2 {
                    log_error("Uso: LOGIN <account_id>");
                    log_error("Ejemplo: LOGIN 72057594037927936");
                    return Self::Null();
                }
                match u64::from_str(&tokens[1]) {
                    Ok(id) => Self::LogIn(id),
                    Err(_) => {
                        log_error("El account_id debe ser un número válido");
                        Self::Null()
                    }
                }
            }
            "LOGOUT" => Self::LogOut(),
            "REG" => parse_register_account(tokens),
            "CARD" => parse_register_card(tokens),
            "UPD" => parse_update(tokens),
            "QUERY" => parse_query(tokens),
            "HELP" => Self::Help(),
            "EXIT" => Self::Exit(),
            _ => Self::Null(),
        }
    }

    /// EL booleano significa hago el wait and print
    pub fn apply(&self, sender: &Sender<Vec<u8>>, id: &mut Option<AccountId>) -> bool {
        match self {
            Command::LogIn(aux_id) => change_login(aux_id, id),
            Command::LogOut() => true,
            Command::RegAcc(username, display_name) => {
                register_new_account(sender, username.to_string(), display_name.to_string())
            }
            Command::UpdRegions(regions, limits) => {
                update_regions(sender, regions.clone(), limits.clone(), id)
            }
            Command::RemoveRegions(regions) => remove_regions(sender, regions.clone(), id),
            Command::RegCard(limit) => register_new_card(sender, *limit, id),
            Command::Upd(new_name, card_id, new_limit) => {
                update_fields(sender, new_name.to_string(), *card_id, *new_limit, id)
            }
            Command::Query(q_type, cid, lower_bound, upper_bound) => {
                query_data(sender, *q_type, *cid, *lower_bound, *upper_bound, id)
            }
            Command::Help() => {
                print_help(id.is_some());
                false
            }
            Command::Null() => {
                log_info("Null command, ignoring...");
                false
            }
            Command::Exit() => {
                log_info("Terminating app...");
                true
            }
        }
    }
}

fn parse_register_account(tokens: Vec<String>) -> Command {
    if tokens.len() != 3 {
        log_error("Uso: REG <username> <display_name>");
        log_error("Ejemplo: REG acme_corp \"Acme Corporation S.A.\"");
        log_error("Nota: username debe ser alfanumérico (3-30 chars)");
        log_error("      display_name puede tener espacios (usa comillas)");
        log_error("      La cuenta se crea SIN regiones. Usa UPD -r para agregar regiones.");
        return Command::Null();
    }

    let username = &tokens[1];
    let display_name = &tokens[2];

    Command::RegAcc(username.clone(), display_name.clone())
}

/// Parser que respeta comillas dobles para tokens con espacios
fn parse_command_with_quotes(input: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current_token = String::new();
    let mut in_quotes = false;
    let chars = input.chars().peekable();

    for ch in chars {
        match ch {
            '"' => {
                in_quotes = !in_quotes;
            }
            ' ' | '\t' => {
                if in_quotes {
                    current_token.push(ch);
                } else if !current_token.is_empty() {
                    tokens.push(current_token.clone());
                    current_token.clear();
                }
            }
            _ => {
                current_token.push(ch);
            }
        }
    }

    // Agregar el último token si existe
    if !current_token.is_empty() {
        tokens.push(current_token);
    }

    tokens
}

fn parse_register_card(tokens: Vec<String>) -> Command {
    if tokens.len() != 2 {
        log_error("Uso: CARD <límite>");
        log_error("Ejemplo: CARD 10000");
        return Command::Null();
    }
    match f64::from_str(&tokens[1]) {
        Ok(limit) => Command::RegCard(limit),
        Err(_) => {
            log_error("El límite debe ser un número válido");
            Command::Null()
        }
    }
}

fn parse_update(tokens: Vec<String>) -> Command {
    if tokens.len() < 2 {
        log_error("Uso: UPD -n <nombre> | UPD -a <límite> | UPD -c <card_id> <límite>");
        return Command::Null();
    }

    match tokens[1].as_str() {
        "-n" => {
            if tokens.len() < 3 {
                log_error("Uso: UPD -n <nuevo_nombre>");
                log_error("Ejemplo: UPD -n \"Mi Nueva Empresa S.A.\"");
                log_error("Nota: Usa comillas si el nombre tiene espacios");
                return Command::Null();
            }
            Command::Upd(tokens[2].clone(), 0u64, 0.0)
        }
        "-a" => {
            if tokens.len() < 3 {
                log_error("Uso: UPD -a <nuevo_límite>");
                log_error("Ejemplo: UPD -a 75000");
                return Command::Null();
            }
            match f64::from_str(&tokens[2]) {
                Ok(new_limit) => Command::Upd("".to_string(), 0u64, new_limit),
                Err(_) => {
                    log_error("El límite debe ser un número válido");
                    Command::Null()
                }
            }
        }
        "-c" => {
            if tokens.len() != 4 {
                log_error("Uso: UPD -c <card_id> <nuevo_límite>");
                log_error("Ejemplo: UPD -c 123456789 15000");
                return Command::Null();
            }
            let card_id = match u64::from_str(&tokens[2]) {
                Ok(id) => id,
                Err(_) => {
                    log_error("El card_id debe ser un número válido");
                    return Command::Null();
                }
            };
            let new_limit = match f64::from_str(&tokens[3]) {
                Ok(limit) => limit,
                Err(_) => {
                    log_error("El límite debe ser un número válido");
                    return Command::Null();
                }
            };
            Command::Upd("".to_string(), card_id, new_limit)
        }
        "-r" => {
            if tokens.len() < 3 {
                log_error("Uso: UPD -r <región1:límite1> [región2:límite2] ...");
                log_error("Ejemplo: UPD -r CABA:50000 SF:30000 Mendoza:25000");
                return Command::Null();
            }

            let mut regions = Vec::new();
            let mut limits = Vec::new();

            for token in tokens.iter().skip(2) {
                let parts: Vec<&str> = token.split(':').collect();
                if parts.len() != 2 {
                    log_error(&format!(
                        "Formato inválido: '{}'. Debe ser región:límite",
                        token
                    ));
                    return Command::Null();
                }

                let region_name = parts[0].to_string();
                let limit = match f64::from_str(parts[1]) {
                    Ok(l) => l,
                    Err(_) => {
                        log_error(&format!(
                            "Límite inválido para región '{}': '{}'",
                            region_name, parts[1]
                        ));
                        return Command::Null();
                    }
                };

                regions.push(region_name);
                limits.push(limit);
            }

            Command::UpdRegions(regions, limits)
        }
        "-d" => {
            if tokens.len() < 3 {
                log_error("Uso: UPD -d <región1> [región2] ...");
                log_error("Ejemplo: UPD -d Mendoza SF");
                return Command::Null();
            }

            let regions = tokens[2..].iter().map(|s| s.to_string()).collect();
            Command::RemoveRegions(regions)
        }
        _ => {
            log_error(
                "Flag inválido. Usa: -n (nombre), -a (account limit), -c (card limit), -r (regiones), -d (remover regiones)",
            );
            Command::Null()
        }
    }
}

fn parse_query(tokens: Vec<String>) -> Command {
    let total = tokens.len();
    if total < 2 {
        log_error("Uso: QUERY -a [fechas] | QUERY -c <card_id> [fechas] | QUERY -d");
        return Command::Null();
    }

    match tokens[1].as_str() {
        "-a" => match total {
            2 => Command::Query(0, 0, TimeStamp::MIN, TimeStamp::MAX),
            3 => match date_to_timestamp(&tokens[2]) {
                Ok(start) => Command::Query(0, 0, start, TimeStamp::MAX),
                Err(_) => {
                    log_error("Formato de fecha inválido. Use: DD-MM-YYYY");
                    log_error("Ejemplo: QUERY -a 01-01-2024");
                    Command::Null()
                }
            },
            4 => match (date_to_timestamp(&tokens[2]), date_to_timestamp(&tokens[3])) {
                (Ok(start), Ok(end)) => Command::Query(0, 0, start, end),
                _ => {
                    log_error("Formato de fecha inválido. Use: DD-MM-YYYY");
                    log_error("Ejemplo: QUERY -a 01-01-2024 31-12-2024");
                    Command::Null()
                }
            },
            _ => {
                log_error("Demasiados argumentos para QUERY -a");
                Command::Null()
            }
        },
        "-c" => {
            if total < 3 {
                log_error("Uso: QUERY -c <card_id> [fecha_inicio] [fecha_fin]");
                log_error("Ejemplo: QUERY -c 123456789");
                return Command::Null();
            }

            let card_id = match CardId::from_str(&tokens[2]) {
                Ok(id) => id,
                Err(_) => {
                    log_error("El card_id debe ser un número válido");
                    return Command::Null();
                }
            };

            match total {
                3 => Command::Query(0, card_id, TimeStamp::MIN, TimeStamp::MAX),
                4 => match date_to_timestamp(&tokens[3]) {
                    Ok(start) => Command::Query(0, card_id, start, TimeStamp::MAX),
                    Err(_) => {
                        log_error("Formato de fecha inválido. Use: DD-MM-YYYY");
                        Command::Null()
                    }
                },
                5 => match (date_to_timestamp(&tokens[3]), date_to_timestamp(&tokens[4])) {
                    (Ok(start), Ok(end)) => Command::Query(0, card_id, start, end),
                    _ => {
                        log_error("Formato de fecha inválido. Use: DD-MM-YYYY");
                        Command::Null()
                    }
                },
                _ => {
                    log_error("Demasiados argumentos para QUERY -c");
                    Command::Null()
                }
            }
        }
        "-d" => Command::Query(1, 0, 0, 0),
        _ => {
            log_error("Flag inválido. Usa: -a (account), -c (card), o -d (detalles)");
            Command::Null()
        }
    }
}

fn date_to_timestamp(fecha_str: &str) -> Result<TimeStamp, chrono::format::ParseError> {
    let format = "%d-%m-%Y";

    let naive_date = NaiveDate::parse_from_str(fecha_str, format)?;

    let datetime_utc = naive_date
        .and_hms_opt(0, 0, 0)
        .expect("WRONG DATE!")
        .and_local_timezone(Utc)
        .unwrap();

    Ok(datetime_utc.timestamp())
}

fn change_login(new_id: &AccountId, account_id: &mut Option<AccountId>) -> bool {
    if let Some(account) = account_id.as_mut() {
        *account = *new_id;
    } else {
        *account_id = Some(*new_id);
    }
    false
}

fn register_new_account(sender: &Sender<Vec<u8>>, username: String, display_name: String) -> bool {
    log_info(&format!(
        "Registrando cuenta '{}' (@{}) sin regiones habilitadas...",
        display_name, username
    ));
    log_info("Usa UPD -r para agregar regiones después.");
    let msg = RegisterAccountMessage::new(username, display_name);
    let framed = frame_payload(msg.serialize());

    sender
        .send(framed)
        .expect("Error when sending data through the channel");
    true
}

fn register_new_card(sender: &Sender<Vec<u8>>, limit: f64, account_id: &Option<AccountId>) -> bool {
    if account_id.is_none() {
        log_error("Error: you are not logged in");
        return false;
    }
    let msg = RegisterCardMessage::new(account_id.unwrap(), limit);
    let framed = frame_payload(msg.serialize());
    sender
        .send(framed)
        .expect("Error when sending data through the channel");
    true
}

fn update_fields(
    sender: &Sender<Vec<u8>>,
    new_name: String,
    card_id: CardId,
    new_limit: f64,
    account_id: &mut Option<AccountId>,
) -> bool {
    if account_id.is_none() {
        log_error("You are not logged");
        return false;
    }
    let msg = UpdateDataMessage::new(account_id.unwrap(), new_name, card_id, new_limit);
    let framed = frame_payload(msg.serialize());
    sender
        .send(framed)
        .expect("Error when sending data through the channel");
    true
}

fn update_regions(
    sender: &Sender<Vec<u8>>,
    regions: Vec<String>,
    limits: Vec<f64>,
    account_id: &Option<AccountId>,
) -> bool {
    if account_id.is_none() {
        log_error("Error: you are not logged in");
        return false;
    }
    log_info(&format!("Actualizando regiones: {:?}", regions));
    let msg = UpdateRegionsMessage::new(account_id.unwrap(), regions, limits);
    let framed = frame_payload(msg.serialize());
    sender
        .send(framed)
        .expect("Error when sending data through the channel");
    true
}

fn remove_regions(
    sender: &Sender<Vec<u8>>,
    regions: Vec<String>,
    account_id: &Option<AccountId>,
) -> bool {
    if account_id.is_none() {
        log_error("Error: you are not logged in");
        return false;
    }
    log_info(&format!("Removiendo regiones: {:?}", regions));
    let msg = RemoveRegionsMessage::new(account_id.unwrap(), regions);
    let framed = frame_payload(msg.serialize());
    sender
        .send(framed)
        .expect("Error when sending data through the channel");
    true
}

fn query_data(
    sender: &Sender<Vec<u8>>,
    q_type: u8,
    card_id: CardId,
    lower_bound: TimeStamp,
    upper_bound: TimeStamp,
    account_id: &mut Option<AccountId>,
) -> bool {
    if account_id.is_none() {
        log_error("You are not logged");
        return false;
    }
    let msg = QueryDataMessage::new(
        q_type,
        account_id.unwrap(),
        card_id,
        lower_bound,
        upper_bound,
    );
    let framed = frame_payload(msg.serialize());
    sender
        .send(framed)
        .expect("Error when sending data through the channel");
    true
}

/// Imprime la ayuda contextual según el estado de login
fn print_help(is_logged_in: bool) {
    println!("\nCOMANDOS DISPONIBLES - COMPANY ADMIN\n");

    if !is_logged_in {
        println!("AUTENTICACIÓN:");
        println!("  REG <username> <display_name>");
        println!("      Registrar nueva cuenta SIN regiones (se hace LOGIN automático)");
        println!("      - username: 3-30 chars alfanumérico (para login)");
        println!("      - display_name: nombre para mostrar (usa comillas si tiene espacios)");
        println!("      - Después usa UPD -r para agregar regiones y límites");
        println!("      Ej: REG acme_corp \"Acme Corporation S.A.\"");
        println!();
        println!("  LOGIN <account_id>");
        println!("      Iniciar sesión con una cuenta existente");
        println!("      Ej: LOGIN 72057594037927936");
        println!();

        println!("Para agregar regiones y crear tarjetas, primero haz LOGIN o REG\n");
    } else {
        println!("GESTIÓN DE TARJETAS:");
        println!("  CARD <límite>");
        println!("      Crear nueva tarjeta asociada a la cuenta actual");
        println!("      Ej: CARD 10000");
        println!();

        println!("ACTUALIZACIÓN:");
        println!("  UPD -n <nuevo_nombre>");
        println!("      Actualizar display name de la cuenta");
        println!("      - Usa comillas si el nombre tiene espacios");
        println!("      Ej: UPD -n \"Nueva Empresa S.A.\"");
        println!();
        println!("  UPD -r <región1:límite1> [región2:límite2] ...");
        println!("      Agregar/actualizar regiones con sus límites");
        println!("      - La cuenta puede operar en múltiples regiones");
        println!("      - Cada región tiene su propio límite independiente");
        println!("      Ej: UPD -r CABA:50000 SF:30000 Mendoza:25000");
        println!();
        println!("  UPD -d <región1> [región2] ...");
        println!("      Remover regiones de la cuenta");
        println!("      Ej: UPD -d Mendoza SF");
        println!();
        println!("  UPD -c <card_id> <nuevo_límite>");
        println!("      Actualizar límite de una tarjeta específica");
        println!("      Ej: UPD -c 123456789 15000");
        println!();

        println!("CONSULTAS:");
        println!("  QUERY -a [fecha_inicio] [fecha_fin]");
        println!("      Ver transacciones de la cuenta");
        println!("      - Sin fechas: todas las transacciones");
        println!("      - Con fecha_inicio: desde esa fecha hasta hoy");
        println!("      - Con ambas fechas: rango específico");
        println!("      Ej: QUERY -a");
        println!("      Ej: QUERY -a 01-01-2024");
        println!("      Ej: QUERY -a 01-01-2024 31-12-2024");
        println!();
        println!("  QUERY -c <card_id> [fecha_inicio] [fecha_fin]");
        println!("      Ver transacciones de una tarjeta específica");
        println!("      Ej: QUERY -c 123456789");
        println!("      Ej: QUERY -c 123456789 01-06-2024 30-06-2024");
        println!();
        println!("  QUERY -d");
        println!("      Ver detalles completos de la cuenta (info + tarjetas)");
        println!("      Ej: QUERY -d");
        println!();
        println!("  Formato de fechas: DD-MM-YYYY (día-mes-año)");
        println!();

        println!("SESIÓN:");
        println!("  LOGOUT  - Cerrar sesión (puedes hacer LOGIN con otra cuenta)");
        println!();
    }

    println!("GENERAL:");
    println!("  HELP    - Mostrar esta ayuda");
    println!("  EXIT    - Cerrar aplicación");
    println!();

    println!("NOTAS:");
    println!("  - Los comandos NO son case-sensitive (HELP = help = Help)");
    println!("  - Los display_name PUEDEN tener espacios (usa comillas dobles)");
    println!("  - Las regiones deben estar registradas en el servidor central");
    println!("  - Una cuenta puede operar en múltiples regiones simultáneamente");
    println!();
}
